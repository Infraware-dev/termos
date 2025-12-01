"""AWS-related tools and utilities.

This module initializes AWS MCP (Model Context Protocol) tools at import time.

Architecture Context:
--------------------
This code runs in a LangGraph API server environment where:
1. The agent is defined at module level and imported by LangGraph's infrastructure
2. We don't control the application entry point (LangGraph CLI starts the server)
3. The agent must be ready immediately when the module is imported

Why asyncio.run() at Module Level:
----------------------------------
MCP clients require async initialization (await client.get_tools()), but Python
doesn't allow 'await' at module level. Since we can't use 'await' directly, we use
asyncio.run() to execute the async initialization synchronously during import.

This approach works because:
- MultiServerMCPClient manages its own connection lifecycle internally
- Unlike the single-client pattern that requires explicit 'async with' context managers,
  MultiServerMCPClient keeps connections alive after initialization
- The tools remain functional throughout the application lifecycle

Alternative approaches considered:
- Lazy initialization: Would require restructuring LangGraph's agent import pattern
- Factory functions: Not compatible with LangGraph's module-level agent imports
- Application startup hooks: Not accessible in LangGraph API server deployment
"""

import asyncio
import logging

from langchain_mcp_adapters.client import MultiServerMCPClient

logger = logging.getLogger(__name__)


def _initialize_mcp_tools():
    """Initialize MCP tools synchronously at module import time.

    This function wraps the async MCP client initialization in a synchronous
    context using asyncio.run(), allowing it to be called at module level.

    Returns:
        list: List of LangChain-compatible tool objects from the AWS MCP server.
              Empty list if initialization fails.

    Raises:
        Exception: Any errors during initialization are caught and logged,
                   returning an empty list to allow graceful degradation.
    """
    # Configure the AWS MCP server connection
    # Uses uvx to run the awslabs.aws-api-mcp-server package
    client = MultiServerMCPClient(
        {
            "aws": {
                "command": "uvx",
                "args": ["awslabs.aws-api-mcp-server@latest"],
                "transport": "stdio",  # Communication via stdin/stdout
            }
        }
    )

    # Execute the async get_tools() call synchronously
    # This blocks during module import, but only happens once
    return asyncio.run(client.get_tools())


# Initialize MCP tools at module import time
# This runs once when the module is first imported by LangGraph
try:
    mcp_tools = _initialize_mcp_tools()
    logger.info("Successfully initialized %d AWS MCP tools", len(mcp_tools))
except Exception as e:
    # Graceful degradation: if MCP initialization fails, agent still works
    # but without MCP tools (will fall back to shell tool only)
    logger.warning("Failed to initialize AWS MCP tools: %s", e)
    mcp_tools = []
