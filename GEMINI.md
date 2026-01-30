# Infraware Terminal

**AI-powered Terminal Emulator for DevOps**

Infraware Terminal is a VTE-based terminal emulator built with Rust and `egui`, featuring an integrated LLM agent for natural language assistance. It allows users to query the terminal using natural language (prefixed with `?`) to generate and execute shell commands.

## 🚀 Project Overview

*   **Core Feature**: Natural language command generation and execution (e.g., `? how do I revert a git commit`).
*   **Architecture**: Monorepo with a Rust workspace containing the terminal client and backend services.
*   **Tech Stack**: Rust (2024 edition), `egui` (GUI), `axum` (Backend API), `rig-rs` (LLM Orchestration), `tokio`.
*   **Current Status**: Transitioning from a Python-based backend to a native Rust backend (`crates/backend-engine`).

## 🏗️ Architecture

The system consists of a terminal client and a backend service that handles LLM interactions.

```mermaid
graph TD
    Client[Terminal App (egui)] <-->|HTTP/SSE| Backend[Backend API (Axum)]
    Backend --> Engine{Agentic Engine}
    
    Engine -->|Native| Rig[RigEngine (Rust + Claude)]
    Engine -->|Testing| Mock[MockEngine]
    Engine -->|Proxy| Http[HttpEngine (LangGraph)]
    
    Rig --> Anthropic[Anthropic API]
```

### Key Components

*   **`terminal-app`**: The frontend terminal emulator using `egui`, `vte`, and `portable-pty`.
*   **`crates/backend-api`**: The REST/SSE server (Axum) acting as the bridge between the terminal and the AI engine.
*   **`crates/backend-engine`**: Defines the `AgenticEngine` trait and implements adapters (Rig, Mock, Http, Process).
*   **`crates/shared`**: Shared types and contracts (Events, Models).

## 🛠️ Getting Started

### Prerequisites (Linux)
```bash
sudo apt install -y pkg-config libssl-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

### 1. Run the Backend
The backend can run with different "Engines".

**Mock Engine (Default - No dependencies):**
```bash
cargo run -p infraware-backend
```

**Rig Engine (Native Rust Agent - Requires API Key):**
```bash
ENGINE_TYPE=rig ANTHROPIC_API_KEY=sk-... cargo run -p infraware-backend --features rig
```

### 2. Run the Terminal
In a separate terminal window:
```bash
cargo run -p infraware-terminal
```

### 3. Usage
In the terminal, type `?` followed by your question:
```bash
? show me running docker containers
```

## 📂 Workspace Structure

| Path | Description |
| :--- | :--- |
| `terminal-app/` | Main GUI application (Rust/egui). |
| `crates/backend-api/` | Backend server exposing HTTP/SSE endpoints. |
| `crates/backend-engine/` | Logic for AI agents (`RigEngine`, `MockEngine`, etc.). |
| `crates/shared/` | Data models and types shared between crate members. |
| `backend/` | Legacy Python backend (LangGraph) - primarily for reference/prototyping. |
| `bin/engine-bridge/` | Python bridge script for `ProcessEngine`. |

## 💻 Development Workflow

**Common Commands:**

```bash
# Build entire workspace
cargo build --workspace

# Run tests
cargo test --workspace

# Linting (Strict)
cargo fmt --all && cargo clippy --workspace -- -D warnings

# Watch mode for backend
cargo watch -x 'run -p infraware-backend'
```

**Coding Standards:**
*   **Microsoft Pragmatic Rust**: Follows guidelines for safety, naming, and error handling.
*   **Error Handling**: Use `anyhow` for apps, `thiserror` for libs.
*   **Commits**: Conventional Commits (e.g., `feat: add new tool`, `fix: resolve crash`).

## 🧠 Key Concepts

### Engines
*   **`RigEngine`**: The primary production engine. Uses `rig-rs` to communicate with Anthropic's Claude. Supports tools (Shell Command, Ask User) and function calling.
*   **`MockEngine`**: Returns hardcoded responses for testing UI flows without API costs.

### HITL (Human-in-the-Loop)
The agent cannot execute commands without user permission.
1.  Agent proposes a command (e.g., `ls -la`).
2.  Backend sends an `Interrupt` event.
3.  Terminal displays the command and asks for Approval (Y/N).
4.  User approves -> Command executes -> Output sent back to Agent (optional).

### `needs_continuation`
A flag in the `ShellCommandTool` that determines if the agent needs to see the output.
*   `false`: The command *is* the answer. Execution finishes the interaction.
*   `true`: The agent needs the output to formulate the final answer (e.g., checking OS version before suggesting install commands).

## ⚙️ Configuration

Set these environment variables (or use `.env`):

```bash
# Backend
ENGINE_TYPE=mock|rig|http|process
ANTHROPIC_API_KEY=sk-...       # Required for RigEngine
PORT=8080
LOG_LEVEL=debug

# Terminal
INFRAWARE_BACKEND_URL=http://localhost:8080
```
