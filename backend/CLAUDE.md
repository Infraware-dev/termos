# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Infraware Terminal is a FastAPI-based web application that wraps a LangGraph supervisor agent system. The supervisor coordinates three specialized agents (AWS, GCP, and command execution) to handle cloud infrastructure tasks and shell commands.

## Key Architecture

### Dual-Server Architecture

The application runs two servers that must start in a specific order:

1. **LangGraph Server** (port 2024): Hosts the supervisor agent and sub-agents
2. **FastAPI Server** (port 8000): Provides authentication and proxies requests to LangGraph

Use `./start-services.sh` to start both servers with proper health checking. The script waits for LangGraph to be ready before starting FastAPI.

### Agent System

The core agent architecture is in `src/agents/`:

- **Supervisor** (`supervisor/agent.py`): Created using `langgraph-supervisor`, orchestrates work between sub-agents. Configured to assign one agent at a time sequentially, never in parallel. Critical: supervisor must be "invisible" - it passes through agent responses without meta-commentary.

- **AWS Agent** (`aws/agent.py`): Uses `deepagents` framework. Accesses AWS via MCP (Model Context Protocol) tools initialized at module import time using `asyncio.run()`. MCP client uses `MultiServerMCPClient` which maintains connection lifecycle internally.

- **GCP Agent** (`gcp/agent.py`): Uses `create_agent` from LangChain. Has custom tools and falls back to shell commands.

- **Command Execution Agent** (`command_execution/agent.py`): Uses `deepagents` framework. Executes shell commands with human approval via `shell_with_approval` tool.

### FastAPI Proxy Layer

Located in `src/api/`:

- `main.py`: FastAPI application entry point
- `routes/langgraph_routes.py`: Proxies all requests to LangGraph server (localhost:2024) with authentication checks
- `routes/auth_routes.py`: Handles Anthropic API key authentication
- `config.py`: Manages `.env` file for API keys
- `auth.py`: Validates Anthropic API keys against live API

### Shared Resources

`src/shared/`:
- `models.py`: Centralized model configuration (claude-sonnet-4-20250514, temperature=0)
- `tools/shell_tool.py`: Shell command execution with LangGraph interrupt-based approval

## Development Commands

### Environment Setup
```bash
pip install -e . "langgraph-cli[inmem]"
cp .env.example .env  # Add ANTHROPIC_API_KEY and LANGSMITH_API_KEY
```

### Running the Application
```bash
# Start both servers with health check (recommended)
./start-services.sh

# Or manually:
langgraph dev              # Start LangGraph server (port 2024)
uv run main.py            # Start FastAPI server (port 8000)
```

### Testing
```bash
# Run all tests
pytest

# Run with coverage
pytest --cov=src --cov-report=html

# Run specific test types
pytest tests/unit/          # Unit tests only
pytest tests/integration/   # Integration tests only

# Run single test file
pytest tests/unit/test_config.py

# Run single test
pytest tests/unit/test_config.py::test_specific_function
```

### Code Quality
```bash
# Lint (check only)
ruff check

# Lint and auto-fix
ruff check --fix

# Format code
ruff format

# Type checking
mypy src/
```

## Important Patterns

### MCP Tool Initialization

AWS agent tools are initialized at module import time (`aws/tools.py`). This is required because:
- LangGraph imports agents at module level
- MCP client needs async initialization but Python doesn't allow `await` at module level
- Solution: `asyncio.run()` wraps async `client.get_tools()` for synchronous execution
- `MultiServerMCPClient` manages connection lifecycle internally, keeping tools functional after initialization

### Agent Communication

All agents must:
- Handle only their domain (AWS/GCP/commands)
- Return results directly to supervisor without extra text
- The supervisor is configured to be "invisible" and pass through responses without commentary

### Authentication Flow

1. Client calls `/api/auth` with Anthropic API key
2. FastAPI validates key against Anthropic API
3. Valid key is stored in `.env` file
4. All subsequent requests check authentication via `check_auth()` before proxying to LangGraph

### LangGraph Configuration

`langgraph.json` defines the graph entry point as `src/agents/supervisor/agent.py:supervisor`. This is what LangGraph CLI loads when running `langgraph dev`.

## Code Style

- Follow Google docstring convention (enforced by ruff)
- Imports sorted and formatted (ruff handles this)
- Type hints required for function signatures
- Python 3.11+ required (uses modern type syntax like `str | None`)
