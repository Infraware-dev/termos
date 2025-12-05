"""Authentication routes for API key management."""

import logging

from fastapi import APIRouter, HTTPException

from ..auth import validate_anthropic_api_key
from ..config import config
from ..models import AuthRequest, AuthResponse, AuthStatusResponse

logger = logging.getLogger(__name__)

router = APIRouter(prefix="/api", tags=["authentication"])


@router.post("/auth", response_model=AuthResponse)
async def authenticate(request: AuthRequest) -> AuthResponse:
    """Authenticate by validating and storing an Anthropic API key.

    Args:
        request: Authentication request containing the API key

    Returns:
        AuthResponse: Authentication result

    Raises:
        HTTPException: If validation or storage fails
    """
    logger.info("=== Authentication request received ===")
    logger.info(f"API key length: {len(request.api_key)}")
    logger.info(
        f"API key prefix: {request.api_key[:15]}..."
        if len(request.api_key) > 15
        else f"API key: {request.api_key}"
    )

    # Validate the API key
    logger.info("Validating API key with Anthropic API...")
    is_valid, message = await validate_anthropic_api_key(request.api_key)

    logger.info(f"Validation result: is_valid={is_valid}, message={message}")

    if not is_valid:
        logger.error(f"API key validation failed: {message}")
        raise HTTPException(status_code=400, detail=message)

    # Store the API key in .env
    if not config.set_api_key(request.api_key):
        raise HTTPException(
            status_code=500, detail="Failed to store API key in configuration"
        )

    return AuthResponse(
        success=True, message="API key validated and stored successfully"
    )


@router.get("/get-auth", response_model=AuthStatusResponse)
async def get_auth_status() -> AuthStatusResponse:
    """Get the current authentication status.

    Returns:
        AuthStatusResponse: Current authentication status
    """
    has_key = config.get_api_key() is not None
    is_auth = config.is_authenticated()

    return AuthStatusResponse(authenticated=is_auth, has_api_key=has_key)
