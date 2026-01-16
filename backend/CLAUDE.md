# CLAUDE.md - Legacy Python Backend

**DEPRECATED**: This Python backend is being replaced by the Rust `infraware-backend` crate.

**Status**: Maintenance mode only. For new development, use the Rust backend with RigEngine.

This file documents the legacy FastAPI application wrapping a LangGraph supervisor agent system (no longer the primary architecture).

## Commands

```bash
# Environment setup
pip install -e . "langgraph-cli[inmem]"
cp .env.example .env  # Add ANTHROPIC_API_KEY and LANGSMITH_API_KEY

# Running (start both servers)
./start-services.sh                    # Recommended: handles health checks
uv run langgraph dev --no-browser      # LangGraph server (port 2024)
uv run main.py                         # FastAPI server (port 8000)

# Testing
pytest                                 # All tests
pytest tests/unit/                     # Unit tests only
pytest tests/integration/              # Integration tests only
pytest tests/unit/test_config.py::test_function  # Single test

# Code quality
ruff check --fix && ruff format        # Lint and format
mypy src/                              # Type checking
```

## Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                     Dual-Server Architecture                         │
├──────────────────────────────────────────────────────────────────────┤
│  FastAPI (port 8000)              LangGraph (port 2024)              │
│  ├── /api/auth (validate keys)    ├── Supervisor Agent              │
│  ├── /health                      │   ├── AWS Agent (MCP tools)     │
│  └── /* (proxy to LangGraph)      │   ├── GCP Agent                 │
│                                   │   └── Command Execution Agent   │
└──────────────────────────────────────────────────────────────────────┘
```

**Startup order matters**: LangGraph must be ready before FastAPI. `start-services.sh` handles this with health checks.

### Key Directories

| Directory | Purpose |
|-----------|---------|
| `src/agents/supervisor/` | LangGraph supervisor using `langgraph-supervisor` |
| `src/agents/aws/` | AWS agent with MCP tools (`deepagents` framework) |
| `src/agents/gcp/` | GCP agent (`create_agent` from LangChain) |
| `src/agents/command_execution/` | Shell command execution with HITL approval |
| `src/api/` | FastAPI app, routes, auth, config |
| `src/shared/` | Shared model config, shell tool |

## Critical Patterns

### MCP Tool Initialization (aws/tools.py)
AWS MCP tools use `asyncio.run()` at module level because:
- LangGraph imports agents at module level (no control over entry point)
- MCP requires async init, but `await` is not allowed at module level
- `MultiServerMCPClient` maintains connections internally after initialization

### Supervisor Invisibility
The supervisor must pass through agent responses without meta-commentary. Never add phrases like "The agent successfully..." - just return the core answer.

### Authentication Flow
1. Client POSTs API key to `/api/auth`
2. Key validated against live Anthropic API
3. Valid key stored in `.env`
4. Subsequent requests checked via `check_auth()` before proxying

## Configuration

| Variable | Purpose |
|----------|---------|
| `ANTHROPIC_API_KEY` | Required for LLM calls |
| `LANGSMITH_API_KEY` | Optional tracing |

Model: `anthropic:claude-sonnet-4-20250514` (temperature=0) defined in `src/shared/models.py`

## Code Style

- Google docstrings (enforced by ruff D-rules)
- Python 3.11+ type syntax (`str | None`)
- Imports sorted by ruff I-rules
