# Repository Guidelines

## Project Structure & Module Organization
- `terminal-app/`: GUI terminal client (egui).
- `crates/`: Rust workspace crates.
- `crates/backend-api/`: REST/SSE server (axum).
- `crates/backend-engine/`: engine trait + adapters.
- `crates/backend-state/`: state persistence.
- `crates/shared/`: shared types.
- `backend/`: legacy Python FastAPI backend.
- `bin/engine-bridge/`: Python bridge for ProcessEngine.
- `docs/`: architecture and getting-started guides.

## Build, Test, and Development Commands
- `cargo build --workspace`: build all Rust crates.
- `cargo test --workspace`: run the full Rust test suite.
- `cargo fmt --all`: format Rust code with rustfmt.
- `cargo clippy --workspace`: lint Rust code.
- `cargo run -p infraware-backend`: start the Rust backend (MockEngine default).
- `ENGINE_TYPE=http LANGGRAPH_URL=http://localhost:2024 cargo run -p infraware-backend`: start backend with HttpEngine (LangGraph proxy).
- `ENGINE_TYPE=process BRIDGE_SCRIPT=bin/engine-bridge/main.py cargo run -p infraware-backend`: start backend with ProcessEngine.
- `ENGINE_TYPE=rig cargo run -p infraware-backend --features rig`: start backend with RigEngine.
- `ENGINE_TYPE=rig cargo run -p infraware-backend --features rig-memory`: start RigEngine with memory enabled.
- `cargo run -p infraware-terminal`: start the terminal client.
- `cargo watch -x 'run -p infraware-backend'`: auto-rebuild backend on changes.

## Coding Style & Naming Conventions
- Rust formatting is enforced with `cargo fmt` (rustfmt).
- Lint with `cargo clippy --workspace` before submitting.
- Use standard Rust naming: `snake_case` for functions/vars, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for constants.
- Keep modules focused and aligned to crate boundaries under `crates/`.

## Testing Guidelines
- Primary test runner is `cargo test --workspace`.
- Add unit tests next to the code they cover (typical Rust `mod tests` style) or in `tests/` where appropriate.
- No explicit coverage target is documented; prioritize meaningful tests for new behavior.

## Commit & Pull Request Guidelines
- Commit messages follow Conventional Commits (e.g., `feat: ...`, `fix: ...`).
- PRs should include a short summary of changes.
- PRs should include how to test (commands and expected outcome).
- PRs should include screenshots or recordings for UI changes in `terminal-app/`.
- PRs should link related issues if applicable.

## Configuration Tips
- Backend environment examples: `ENGINE_TYPE=mock|http|process|rig`, `PORT=8080`, `API_KEY`, `RATE_LIMIT_RPM`.
- For `http`/`process` engines, configure `LANGGRAPH_URL`.
- ProcessEngine can use `BRIDGE_SCRIPT=bin/engine-bridge/main.py`.
- RigEngine requires `ANTHROPIC_API_KEY`; optional memory config includes `MEMORY_ENABLED`, `MEMORY_DATA_DIR`, `MEMORY_MAX_RESULTS`.
- Terminal config: `INFRAWARE_BACKEND_URL=http://localhost:8080`.
- See `README.md` and `docs/GETTING_STARTED.md` for full setup details.
