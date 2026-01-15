# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Infraware Terminal** is a next-generation AI-powered terminal emulator for DevOps engineers. It consists of two major components in a monorepo:

1. **Terminal Application** (`terminal-app/`): Rust-based egui terminal emulator with VTE parsing and PTY management
2. **Backend Services** (`backend/`): Python-based LangGraph supervisor agent system with FastAPI proxy

The terminal application provides native terminal emulation with an integrated LLM agent that assists when commands fail or when users need expert guidance. The backend coordinates specialized agents (AWS, GCP, command execution) through a supervisor architecture.

## Repository Structure

```
infraware-terminal/
├── backend/              # Python LangGraph backend
│   ├── src/
│   │   ├── agents/      # Supervisor + AWS/GCP/Command agents
│   │   ├── api/         # FastAPI proxy and authentication
│   │   └── shared/      # Shared models and tools
│   ├── tests/           # Unit and integration tests
│   ├── pyproject.toml   # Python dependencies (uv)
│   └── langgraph.json   # LangGraph configuration
├── terminal-app/        # Rust terminal emulator
│   ├── src/
│   │   ├── terminal/    # VTE parser, grid, cell attributes
│   │   ├── pty/         # PTY session management
│   │   ├── llm/         # LLM client with SSE streaming
│   │   ├── orchestrators/ # Natural language + HITL workflows
│   │   ├── input/       # Keyboard, selection, validation
│   │   └── ui/          # egui rendering, theme, scrollbar
│   ├── Cargo.toml       # Rust dependencies
│   └── deny.toml        # cargo-deny configuration
├── docs/                # Documentation
├── examples/            # Example files
└── install.sh           # Setup script for backend

```

**Important**: Each component has its own detailed CLAUDE.md file:
- `backend/CLAUDE.md` - Python backend architecture and commands
- `terminal-app/CLAUDE.md` - Rust terminal application details

## Quick Start Commands

### Initial Setup

```bash
# Install backend dependencies
./install.sh

# Or manually:
cd backend
pip install -e . "langgraph-cli[inmem]"
cp .env.example .env  # Add ANTHROPIC_API_KEY and LANGSMITH_API_KEY

# Build Rust terminal (Linux prerequisites)
sudo apt install -y pkg-config libssl-dev
cd terminal-app
cargo build --release
```

### Running the Application

```bash
# Backend: Start both LangGraph and FastAPI servers
cd backend
./start-services.sh                    # Recommended (handles health checks)
# OR manually:
langgraph dev                          # LangGraph server (port 2024)
uv run main.py                         # FastAPI server (port 8000)

# Terminal: Run the terminal emulator
cd terminal-app
cargo run                              # Development build
cargo run --release                    # Production build
LOG_LEVEL=debug cargo run              # With debug logging
```

### Testing

```bash
# Backend (Python)
cd backend
pytest                                 # All tests
pytest tests/unit/                     # Unit tests only
pytest tests/integration/              # Integration tests only
pytest --cov=src --cov-report=html     # With coverage

# Terminal (Rust)
cd terminal-app
cargo test                             # All tests (36 tests)
cargo test test_name                   # Single test
cargo test -- --nocapture              # With output
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info  # Coverage
```

### Code Quality

```bash
# Backend (Python)
cd backend
ruff check                             # Lint (check only)
ruff check --fix                       # Lint and auto-fix
ruff format                            # Format code
mypy src/                              # Type checking

# Terminal (Rust)
cd terminal-app
cargo fmt                              # Format code
cargo clippy                           # Lint (must pass with 0 warnings)
cargo check                            # Fast type check
```

## Architecture Overview

### System Communication Flow

```
┌─────────────────────────────────────────────────────────────┐
│                    Terminal Emulator (Rust)                 │
│  ┌───────────────┐  ┌──────────────┐  ┌─────────────────┐  │
│  │ Shell Process │  │ LLM Client   │  │ Response        │  │
│  │ (PTY)         │  │ (HTTP/SSE)   │  │ Renderer        │  │
│  └───────┬───────┘  └──────┬───────┘  └────────┬────────┘  │
│          │                  │                    │           │
│          │                  │                    │           │
└──────────┼──────────────────┼────────────────────┼───────────┘
           │                  │                    │
           │                  ▼                    │
           │       ┌────────────────────┐         │
           │       │   FastAPI Server   │         │
           │       │   (Port 8000)      │         │
           │       │ • Authentication   │         │
           │       │ • Request Proxy    │         │
           │       └─────────┬──────────┘         │
           │                 │                     │
           │                 ▼                     │
           │       ┌────────────────────┐         │
           │       │  LangGraph Server  │         │
           │       │  (Port 2024)       │         │
           │       │ ┌────────────────┐ │         │
           │       │ │  Supervisor    │ │         │
           │       │ └───┬────────────┘ │         │
           │       │     │               │         │
           │       │ ┌───┴───────────┐  │         │
           │       │ │ AWS   GCP     │  │         │
           │       │ │ Agent Agent   │  │         │
           │       │ │               │  │         │
           │       │ │ Command Agent │  │         │
           │       │ └───────────────┘  │         │
           │       └─────────┬──────────┘         │
           │                 │                     │
           └─────────────────┴─────────────────────┘
                       Execution
```

### Component Interaction

1. **User Input**: User types in Rust terminal (either shell commands or `?` magic input)
2. **Shell Processing**: Regular commands go to PTY, magic input (`?`) triggers LLM query
3. **LLM Query**: Terminal sends HTTP/SSE request to FastAPI backend
4. **Authentication**: FastAPI validates Anthropic API key before proxying
5. **Agent Orchestration**: LangGraph supervisor delegates to AWS/GCP/Command agents
6. **Human-in-the-Loop**: Agents request approval for commands or ask questions
7. **Response Streaming**: SSE streams responses back to terminal
8. **Rendering**: Terminal converts markdown to ANSI with syntax highlighting

## Key Design Patterns

### Backend (Python)

- **Supervisor Pattern**: LangGraph supervisor orchestrates specialized agents sequentially (never parallel)
- **MCP Tool Initialization**: AWS agent uses Model Context Protocol tools initialized at module import with `asyncio.run()`
- **Proxy Architecture**: FastAPI proxies all requests to LangGraph with authentication checks
- **Interrupt-Based Approval**: Shell commands use LangGraph interrupts for human approval

### Terminal (Rust)

- **State Machine**: `AppMode` enum with validated transitions (Normal → WaitingLLM → AwaitingApproval → Normal)
- **Dependency Injection**: Traits for PTY and LLM client enable testing with mocks
- **Single-Pass Rendering**: egui rendering batches backgrounds, text, and decorations in one pass
- **Orchestrator Pattern**: Separate orchestrators for natural language queries and HITL workflows
- **SSE Streaming**: Real-time LLM responses via Server-Sent Events

## Development Workflow

### When Working on Backend

1. Navigate to `backend/` directory
2. Consult `backend/CLAUDE.md` for detailed architecture
3. Start LangGraph server first (`langgraph dev`), then FastAPI (`uv run main.py`)
4. Run tests with `pytest` before committing
5. Use `ruff check --fix` and `ruff format` for code quality

### When Working on Terminal

1. Navigate to `terminal-app/` directory
2. Consult `terminal-app/CLAUDE.md` for detailed architecture
3. Run `cargo fmt && cargo clippy` before committing (CI enforces 0 warnings)
4. Use `cargo test` to verify changes
5. Follow Microsoft Pragmatic Rust Guidelines (all public types implement `Debug`, use `#[expect]` for lints)

### Cross-Component Changes

When changes span both backend and terminal:

1. **API Contract Changes**: Update both LLM client (`terminal-app/src/llm/client.rs`) and FastAPI routes (`backend/src/api/routes/`)
2. **Authentication Flow**: Coordinate changes between `terminal-app/src/auth/` and `backend/src/api/auth.py`
3. **Command Safety**: Ensure validation patterns in `terminal-app/src/input/command_validator.rs` align with backend expectations
4. **Error Messages**: Keep error messages consistent between Rust and Python components

## Environment Configuration

### Backend (.env in backend/)

```bash
ANTHROPIC_API_KEY=sk-ant-xxx        # Required for LLM
LANGSMITH_API_KEY=lsv2_xxx          # Optional for tracing
```

### Terminal (.env in terminal-app/)

```bash
INFRAWARE_BACKEND_URL=http://localhost:8000   # FastAPI backend URL
ANTHROPIC_API_KEY=sk-ant-xxx                  # API key for authentication
LOG_LEVEL=debug                               # Logging level (debug/info/warn/error)
```

**Note**: Terminal also supports `.env.secrets` (gitignored) for API keys to avoid accidental commits.

## Git Workflow

### Commit Message Format

Use conventional commit format for both components:

```
<type>: <description>

Types: feat, fix, refactor, docs, test, chore, perf, style
```

**Important**:
- Maximum 50 characters for subject line
- Use imperative mood ("Add" not "Added")
- **NO** emojis, Co-Authored-By, or AI attribution in commits
- Run code quality tools before committing

### Branch Strategy

- `main`: Production-ready code
- `feat/*`: Feature branches
- Current branch: `feat/new-terminal`

## Common Tasks

### Add New LLM Agent to Backend

1. Create agent in `backend/src/agents/<agent_name>/`
2. Implement agent with appropriate framework (deepagents or LangChain)
3. Add agent to supervisor configuration in `backend/src/agents/supervisor/agent.py`
4. Write tests in `backend/tests/unit/test_<agent_name>.py`

### Add Keyboard Shortcut to Terminal

1. Edit `terminal-app/src/input/keyboard.rs`
2. Add to `process_ctrl_keys()` (Ctrl combos) or `process_other_keys()` (special keys)
3. Return `KeyboardAction::SendBytes(vec![...])` with appropriate bytes

### Modify Terminal Rendering

1. Edit `terminal-app/src/app.rs` → `render_terminal()`
2. Maintain single-pass rendering (pre-allocate buffers, no nested loops)
3. Use `column_x_coords` cache for X position lookups

### Add VTE Escape Sequence

1. Edit `terminal-app/src/terminal/handler.rs`
2. Add match arm in `csi_dispatch()` (CSI sequences) or `esc_dispatch()` (ESC sequences)
3. Update grid state via `self.grid` methods

## Testing Strategy

### Backend Testing

- **Unit Tests**: Test individual functions and classes in isolation
- **Integration Tests**: Test agent interactions and API endpoints
- **Coverage Target**: Maintain >80% coverage
- Use pytest fixtures for shared test resources

### Terminal Testing

- **Unit Tests**: Test individual modules with mocks via DI traits
- **Integration Tests**: Test PTY, VTE parsing, LLM client with real backends
- **Current Status**: 36 tests, focus on critical paths (VTE, PTY, state machine)
- **Known Gap**: LLM orchestration needs more unit tests

## Security Considerations

### Command Validation

Terminal validates all LLM-suggested commands before execution (`terminal-app/src/input/command_validator.rs`):

- **Blocked**: `rm -rf /`, `mkfs`, `dd if=/dev/zero`, fork bombs, remote code execution, data exfiltration
- **Warning**: `rm -rf ./...`, system permission changes, shutdown/reboot

### Authentication

- All backend requests require valid Anthropic API key
- Keys validated against live Anthropic API before storage
- FastAPI uses `check_auth()` dependency for protected routes

### Best Practices

- Never log API keys or sensitive data
- Use `.env.secrets` (gitignored) for local secrets
- Validate all user input before PTY execution
- Sanitize LLM responses before rendering

## Troubleshooting

### Backend Issues

**LangGraph server fails to start**:
- Check `ANTHROPIC_API_KEY` in `backend/.env`
- Verify port 2024 is not in use: `lsof -i :2024`
- Check logs in `backend/logs/`

**FastAPI authentication errors**:
- Verify API key with: `curl -X POST http://localhost:8000/api/auth -H "Content-Type: application/json" -d '{"api_key":"your-key"}'`
- Check `backend/.env` file has correct key

### Terminal Issues

**Terminal won't connect to backend**:
- Verify `INFRAWARE_BACKEND_URL` in `terminal-app/.env`
- Check backend is running: `curl http://localhost:8000/health`
- Terminal falls back to MockLLMClient if backend unavailable

**Rendering issues**:
- Check terminal size with `LOG_LEVEL=debug cargo run`
- Verify VTE parsing with debug logs
- Check `MAX_BYTES_PER_FRAME` if output is truncated

**PTY issues**:
- Verify shell is in PATH: `which bash`
- Check PTY permissions (Linux requires proper user permissions)
- Review backpressure logs if output is delayed

## Performance Optimization

### Backend

- LangGraph server uses async I/O for agent orchestration
- FastAPI proxies requests with minimal overhead
- MCP client maintains persistent connections for tools

### Terminal

- Single-pass rendering keeps CPU <5% when idle
- Reactive repaint only when needed (cursor blink, output received)
- Backpressure limits PTY output to `MAX_BYTES_PER_FRAME` (4096 bytes) per frame
- Pre-allocated buffers for rendering (no allocations in hot path)

## Additional Resources

- Backend Details: `backend/CLAUDE.md`
- Terminal Details: `terminal-app/CLAUDE.md`
- LangGraph Docs: https://langchain-ai.github.io/langgraph/
- egui Docs: https://docs.rs/egui/
- VTE Spec: https://vt100.net/docs/vt100-ug/
