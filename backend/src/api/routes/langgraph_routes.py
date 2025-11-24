"""Reverse proxy routes for LanGraph endpoints."""

import httpx
from fastapi import APIRouter, HTTPException, Request, Response
from fastapi.responses import StreamingResponse

from ..config import config

router = APIRouter(tags=["langgraph"])

# LanGraph server configuration
LANGGRAPH_SERVER_URL = "http://localhost:2024"


async def check_auth():
    """Check if user is authenticated.

    Raises:
        HTTPException: If user is not authenticated
    """
    if not config.is_authenticated():
        raise HTTPException(
            status_code=401,
            detail="Not authenticated. Please provide an API key via /api/auth",
        )


async def proxy_request(
    request: Request, path: str, method: str = "GET"
) -> Response:
    """Proxy a request to the LanGraph server.

    Args:
        request: The incoming FastAPI request
        path: The path to proxy to
        method: HTTP method to use

    Returns:
        Response: The proxied response

    Raises:
        HTTPException: If the proxy request fails
    """
    # Build the target URL
    target_url = f"{LANGGRAPH_SERVER_URL}{path}"

    # Get query parameters
    query_params = dict(request.query_params)

    # Get request body if present
    body = None
    if method in ["POST", "PUT", "PATCH"]:
        body = await request.body()

    # Get headers (exclude host header)
    headers = dict(request.headers)
    headers.pop("host", None)

    try:
        async with httpx.AsyncClient(timeout=300.0) as client:
            response = await client.request(
                method=method,
                url=target_url,
                params=query_params,
                content=body,
                headers=headers,
            )

            # Return the response
            return Response(
                content=response.content,
                status_code=response.status_code,
                headers=dict(response.headers),
            )

    except httpx.ConnectError:
        raise HTTPException(
            status_code=503,
            detail="LanGraph server is not running. Please start it with 'langgraph dev'",
        )
    except httpx.TimeoutException:
        raise HTTPException(status_code=504, detail="Request to LanGraph server timed out")
    except Exception as e:
        raise HTTPException(
            status_code=500, detail=f"Proxy error: {str(e)}"
        )


async def proxy_streaming_request(request: Request, path: str) -> StreamingResponse:
    """Proxy a streaming request to the LanGraph server.

    Args:
        request: The incoming FastAPI request
        path: The path to proxy to

    Returns:
        StreamingResponse: The proxied streaming response

    Raises:
        HTTPException: If the proxy request fails
    """
    target_url = f"{LANGGRAPH_SERVER_URL}{path}"
    query_params = dict(request.query_params)
    body = await request.body()
    headers = dict(request.headers)
    headers.pop("host", None)

    try:
        client = httpx.AsyncClient(timeout=300.0)

        async def generate():
            try:
                async with client.stream(
                    "POST",
                    target_url,
                    params=query_params,
                    content=body,
                    headers=headers,
                ) as response:
                    async for chunk in response.aiter_bytes():
                        yield chunk
            finally:
                await client.aclose()

        return StreamingResponse(
            generate(),
            media_type="text/event-stream",
        )

    except httpx.ConnectError:
        raise HTTPException(
            status_code=503,
            detail="LanGraph server is not running. Please start it with 'langgraph dev'",
        )
    except Exception as e:
        raise HTTPException(
            status_code=500, detail=f"Streaming proxy error: {str(e)}"
        )


# Proxy endpoints for LanGraph API
@router.post("/runs/stream")
async def stream_run(request: Request):
    """Stream a run from the LanGraph supervisor.

    Args:
        request: The incoming request

    Returns:
        StreamingResponse: Streamed response from LanGraph
    """
    await check_auth()
    return await proxy_streaming_request(request, "/runs/stream")


@router.post("/runs/invoke")
async def invoke_run(request: Request):
    """Invoke a synchronous run from the LanGraph supervisor.

    Args:
        request: The incoming request

    Returns:
        Response: Response from LanGraph
    """
    await check_auth()
    return await proxy_request(request, "/runs/invoke", method="POST")


@router.get("/runs/{run_id}")
async def get_run(request: Request, run_id: str):
    """Get the status of a specific run.

    Args:
        request: The incoming request
        run_id: The run ID

    Returns:
        Response: Run status from LanGraph
    """
    await check_auth()
    return await proxy_request(request, f"/runs/{run_id}", method="GET")


@router.post("/threads")
async def create_thread(request: Request):
    """Create a new conversation thread.

    Args:
        request: The incoming request

    Returns:
        Response: Thread creation response from LanGraph
    """
    await check_auth()
    return await proxy_request(request, "/threads", method="POST")


@router.get("/threads/{thread_id}")
async def get_thread(request: Request, thread_id: str):
    """Get a conversation thread.

    Args:
        request: The incoming request
        thread_id: The thread ID

    Returns:
        Response: Thread data from LanGraph
    """
    await check_auth()
    return await proxy_request(request, f"/threads/{thread_id}", method="GET")


@router.get("/threads/{thread_id}/history")
async def get_thread_history(request: Request, thread_id: str):
    """Get the history of a conversation thread.

    Args:
        request: The incoming request
        thread_id: The thread ID

    Returns:
        Response: Thread history from LanGraph
    """
    await check_auth()
    return await proxy_request(
        request, f"/threads/{thread_id}/history", method="GET"
    )


# Catch-all proxy for other LanGraph endpoints
@router.api_route("/{path:path}", methods=["GET", "POST", "PUT", "DELETE", "PATCH"])
async def proxy_other(request: Request, path: str):
    """Proxy all other requests to LanGraph server.

    Args:
        request: The incoming request
        path: The path to proxy

    Returns:
        Response: Response from LanGraph
    """
    await check_auth()
    return await proxy_request(request, f"/{path}", method=request.method)
