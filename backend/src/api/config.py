"""Configuration management for FastAPI application."""

import os
from pathlib import Path

from dotenv import load_dotenv, set_key


class Config:
    """Application configuration manager."""

    def __init__(self):
        """Initialize configuration."""
        self.backend_dir = Path(__file__).parent.parent.parent
        self.env_file = self.backend_dir / ".env"
        load_dotenv(self.env_file)

    def get_api_key(self) -> str | None:
        """Get the Anthropic API key from environment.

        Returns:
            str | None: API key if set, None otherwise
        """
        return os.getenv("ANTHROPIC_API_KEY")

    def set_api_key(self, api_key: str) -> bool:
        """Set the Anthropic API key in .env file.

        Args:
            api_key: The API key to set

        Returns:
            bool: True if successful, False otherwise
        """
        try:
            # Create .env file if it doesn't exist
            if not self.env_file.exists():
                self.env_file.touch()

            # Update the .env file
            set_key(str(self.env_file), "ANTHROPIC_API_KEY", api_key)

            # Update the current environment
            os.environ["ANTHROPIC_API_KEY"] = api_key

            return True
        except Exception:
            return False

    def is_authenticated(self) -> bool:
        """Check if user is authenticated (has API key configured).

        Returns:
            bool: True if API key is set, False otherwise
        """
        api_key = self.get_api_key()
        return api_key is not None and len(api_key.strip()) > 0


# Global config instance
config = Config()
