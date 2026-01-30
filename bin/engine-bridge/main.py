#!/usr/bin/env python3
"""
Engine Bridge - JSON-RPC bridge for LangGraph

This script provides a stdio-based JSON-RPC interface to a LangGraph server.
It receives requests on stdin and sends responses to stdout.

Protocol:
- Line-delimited JSON-RPC 2.0
- Supports streaming via event messages

Methods:
- health_check: Check connection to LangGraph
- create_thread: Create a new conversation thread
- stream_run: Start a streaming run with user input
- resume_run: Resume after an interrupt (HITL)
"""

import asyncio
import json
import os
import sys
from typing import Any

import httpx

# Configuration from environment
LANGGRAPH_URL = os.environ.get("LANGGRAPH_URL", "http://localhost:2024")
DEBUG = os.environ.get("DEBUG", "0") == "1"


def debug(msg: str) -> None:
    """Write debug message to stderr."""
    if DEBUG:
        print(f"[DEBUG] {msg}", file=sys.stderr)


def send_response(request_id: str, result: Any = None, error: dict | None = None) -> None:
    """Send a JSON-RPC response."""
    response = {
        "jsonrpc": "2.0",
        "id": request_id,
    }
    if error:
        response["error"] = error
    else:
        response["result"] = result

    print(json.dumps(response), flush=True)


def send_event(request_id: str, event: dict) -> None:
    """Send a JSON-RPC event (streaming response)."""
    response = {
        "jsonrpc": "2.0",
        "id": request_id,
        "event": event,
    }
    print(json.dumps(response), flush=True)


async def health_check(client: httpx.AsyncClient, request_id: str, _params: dict) -> None:
    """Check LangGraph server health."""
    try:
        response = await client.get(f"{LANGGRAPH_URL}/ok")
        if response.status_code == 200:
            send_response(request_id, {"status": "healthy", "langgraph_url": LANGGRAPH_URL})
        else:
            send_response(request_id, error={
                "code": -32000,
                "message": f"LangGraph returned status {response.status_code}"
            })
    except httpx.ConnectError as e:
        send_response(request_id, error={
            "code": -32002,
            "message": f"Cannot connect to LangGraph at {LANGGRAPH_URL}: {e}"
        })


async def create_thread(client: httpx.AsyncClient, request_id: str, params: dict) -> None:
    """Create a new conversation thread."""
    try:
        metadata = params.get("metadata")
        response = await client.post(
            f"{LANGGRAPH_URL}/threads",
            json={"metadata": metadata} if metadata else {}
        )

        if response.status_code == 200:
            data = response.json()
            send_response(request_id, {"thread_id": data.get("thread_id")})
        else:
            send_response(request_id, error={
                "code": -32000,
                "message": f"Failed to create thread: {response.text}"
            })
    except httpx.ConnectError as e:
        send_response(request_id, error={
            "code": -32002,
            "message": f"Connection error: {e}"
        })


async def stream_run(client: httpx.AsyncClient, request_id: str, params: dict) -> None:
    """Start a streaming run."""
    thread_id = params.get("thread_id")
    input_data = params.get("input", {})

    if not thread_id:
        send_response(request_id, error={
            "code": -32602,
            "message": "thread_id is required"
        })
        return

    try:
        # Build the request
        request_body = {
            "assistant_id": "supervisor",
            "stream_mode": ["values", "updates", "messages"],
            "input": input_data,
        }

        debug(f"stream_run to {LANGGRAPH_URL}/threads/{thread_id}/runs/stream")

        async with client.stream(
            "POST",
            f"{LANGGRAPH_URL}/threads/{thread_id}/runs/stream",
            json=request_body,
            timeout=300.0
        ) as response:
            if response.status_code != 200:
                error_text = await response.aread()
                send_response(request_id, error={
                    "code": -32000,
                    "message": f"Stream run failed ({response.status_code}): {error_text.decode()}"
                })
                return

            # Parse SSE events
            current_event = None
            async for line in response.aiter_lines():
                line = line.strip()
                if not line:
                    continue

                if line.startswith("event: "):
                    current_event = line[7:].strip()
                elif line.startswith("data: ") and current_event:
                    data = line[6:].strip()
                    try:
                        parsed_data = json.loads(data)
                        event = convert_sse_event(current_event, parsed_data)
                        if event:
                            send_event(request_id, event)
                    except json.JSONDecodeError:
                        debug(f"Failed to parse SSE data: {data}")

        # Send end event
        send_event(request_id, {"type": "end"})

    except httpx.ConnectError as e:
        send_response(request_id, error={
            "code": -32002,
            "message": f"Connection error: {e}"
        })
    except Exception as e:
        send_response(request_id, error={
            "code": -32000,
            "message": f"Stream error: {e}"
        })


async def resume_run(client: httpx.AsyncClient, request_id: str, params: dict) -> None:
    """Resume a run after an interrupt."""
    thread_id = params.get("thread_id")
    response_data = params.get("response", {})

    if not thread_id:
        send_response(request_id, error={
            "code": -32602,
            "message": "thread_id is required"
        })
        return

    try:
        # Build the resume request
        request_body = {
            "assistant_id": "supervisor",
            "stream_mode": ["values", "updates", "messages"],
            "command": {"resume": "approved"},
        }

        # If it's an answer, include user input
        if "answer" in response_data:
            request_body["input"] = {
                "messages": [{"role": "user", "content": response_data["answer"]}]
            }

        debug(f"resume_run to {LANGGRAPH_URL}/threads/{thread_id}/runs/stream")

        async with client.stream(
            "POST",
            f"{LANGGRAPH_URL}/threads/{thread_id}/runs/stream",
            json=request_body,
            timeout=300.0
        ) as response:
            if response.status_code != 200:
                error_text = await response.aread()
                send_response(request_id, error={
                    "code": -32000,
                    "message": f"Resume failed ({response.status_code}): {error_text.decode()}"
                })
                return

            # Parse SSE events
            current_event = None
            async for line in response.aiter_lines():
                line = line.strip()
                if not line:
                    continue

                if line.startswith("event: "):
                    current_event = line[7:].strip()
                elif line.startswith("data: ") and current_event:
                    data = line[6:].strip()
                    try:
                        parsed_data = json.loads(data)
                        event = convert_sse_event(current_event, parsed_data)
                        if event:
                            send_event(request_id, event)
                    except json.JSONDecodeError:
                        debug(f"Failed to parse SSE data: {data}")

        # Send end event
        send_event(request_id, {"type": "end"})

    except httpx.ConnectError as e:
        send_response(request_id, error={
            "code": -32002,
            "message": f"Connection error: {e}"
        })
    except Exception as e:
        send_response(request_id, error={
            "code": -32000,
            "message": f"Resume error: {e}"
        })


def convert_sse_event(event_type: str, data: dict) -> dict | None:
    """Convert an SSE event to our JSON-RPC event format."""
    if event_type == "metadata":
        return {
            "type": "metadata",
            "run_id": data.get("run_id", "unknown"),
        }
    elif event_type == "values":
        messages = data.get("messages", [])
        if messages:
            # Convert messages to our format
            converted = []
            for msg in messages:
                role = msg.get("type")
                if role == "ai":
                    role = "assistant"
                elif role == "human":
                    role = "user"
                else:
                    role = msg.get("role", role)

                content = extract_content(msg)
                if content:
                    converted.append({"role": role, "content": content})

            if converted:
                return {"type": "values", "messages": converted}
    elif event_type == "updates":
        # Check for interrupts
        interrupts = data.get("__interrupt__", [])
        if interrupts:
            converted = []
            for interrupt in interrupts:
                value = interrupt.get("value", {})
                if "command" in value:
                    converted.append({
                        "command": value.get("command"),
                        "message": value.get("message", "Command requires approval"),
                    })
                else:
                    converted.append({
                        "question": value.get("question", value.get("message", "Input needed")),
                        "options": value.get("options"),
                    })
            if converted:
                return {"type": "updates", "interrupts": converted}
    elif event_type == "error":
        return {
            "type": "error",
            "message": data.get("message", "Unknown error"),
        }
    elif event_type == "end":
        return {"type": "end"}

    return None


def extract_content(msg: dict) -> str | None:
    """Extract content from a message (handles string and array formats)."""
    content = msg.get("content")

    if isinstance(content, str):
        return content if content else None

    if isinstance(content, list):
        # Array of content blocks
        text_parts = []
        for block in content:
            if isinstance(block, dict) and block.get("type") == "text":
                text = block.get("text", "")
                if text:
                    text_parts.append(text)
        return "\n".join(text_parts) if text_parts else None

    return None


# Method dispatch table
METHODS = {
    "health_check": health_check,
    "create_thread": create_thread,
    "stream_run": stream_run,
    "resume_run": resume_run,
}


async def handle_request(client: httpx.AsyncClient, request: dict) -> None:
    """Handle a single JSON-RPC request."""
    request_id = request.get("id", "unknown")
    method = request.get("method")
    params = request.get("params", {})

    if method not in METHODS:
        send_response(request_id, error={
            "code": -32601,
            "message": f"Method not found: {method}"
        })
        return

    debug(f"Handling {method} with id={request_id}")
    await METHODS[method](client, request_id, params)


async def main() -> None:
    """Main entry point - read requests from stdin."""
    debug(f"Engine bridge started, connecting to {LANGGRAPH_URL}")

    async with httpx.AsyncClient() as client:
        for line in sys.stdin:
            line = line.strip()
            if not line:
                continue

            try:
                request = json.loads(line)
                debug(f"Received: {request.get('method', 'unknown')}")
                await handle_request(client, request)
            except json.JSONDecodeError as e:
                debug(f"Invalid JSON: {e}")
                send_response("unknown", error={
                    "code": -32700,
                    "message": f"Parse error: {e}"
                })


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        debug("Bridge interrupted")
    except Exception as e:
        print(f"Fatal error: {e}", file=sys.stderr)
        sys.exit(1)
