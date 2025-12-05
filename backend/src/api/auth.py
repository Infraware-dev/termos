"""Authentication utilities for API key validation."""

import logging

import httpx

logger = logging.getLogger(__name__)


async def validate_anthropic_api_key(api_key: str) -> tuple[bool, str]:
    """Validate an Anthropic API key by making a test request.

    Args:
        api_key: The API key to validate

    Returns:
        tuple[bool, str]: (is_valid, error_message)
    """
    logger.info("Starting API key validation...")

    if not api_key or len(api_key.strip()) == 0:
        logger.warning("API key is empty")
        return False, "API key cannot be empty"

    # Basic format validation
    if not api_key.startswith("sk-ant-"):
        logger.warning(f"API key format invalid. Starts with: {api_key[:10]}...")
        return False, "Invalid API key format. Key should start with 'sk-ant-'"

    # Test the API key with a minimal request
    try:
        logger.info("Making test request to Anthropic API...")
        async with httpx.AsyncClient(timeout=10.0) as client:
            response = await client.post(
                "https://api.anthropic.com/v1/messages",
                headers={
                    "x-api-key": api_key,
                    "anthropic-version": "2023-06-01",
                    "content-type": "application/json",
                },
                json={
                    "model": "claude-haiku-4-5-20251001",
                    "max_tokens": 1,
                    "messages": [{"role": "user", "content": "test"}],
                },
            )

            logger.info(f"Anthropic API response status: {response.status_code}")

            if response.status_code == 200:
                logger.info("API key validated successfully")
                return True, "API key is valid"
            elif response.status_code == 401:
                logger.warning("API key rejected by Anthropic (401)")
                error_body = response.text
                logger.warning(f"Error response: {error_body}")
                return False, "Invalid API key"
            elif response.status_code == 429:
                # Rate limited, but key is valid
                logger.info("API key valid but rate limited")
                return True, "API key is valid (rate limited)"
            else:
                error_body = response.text
                logger.warning(
                    f"Validation failed with status {response.status_code}: {error_body}"
                )
                return (
                    False,
                    f"Validation failed with status {response.status_code}: {error_body[:200]}",
                )

    except httpx.TimeoutException:
        logger.error("Timeout while validating API key")
        return False, "Request timed out while validating API key"
    except httpx.RequestError as e:
        logger.error(f"Network error during validation: {str(e)}")
        return False, f"Network error during validation: {str(e)}"
    except Exception as e:
        logger.error(f"Unexpected error during validation: {str(e)}", exc_info=True)
        return False, f"Unexpected error during validation: {str(e)}"
