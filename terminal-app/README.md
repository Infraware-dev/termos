# Infraware Terminal

**A next-generation AI-powered terminal emulator for DevOps engineers.**

Infraware Terminal is a hybrid terminal emulator built with Rust and `egui`. It combines a robust, standards-compliant PTY terminal with an integrated LLM agent that assists you when commands fail or when you need expert guidance.

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Status](https://img.shields.io/badge/status-beta-orange.svg)

## 🚀 Key Features

*   **Native Terminal Emulation:** Full support for interactive CLI tools (`vim`, `htop`, `ssh`) via `portable-pty` and `vte`.
*   **AI Shell Hook:** Automatically detects "command not found" errors and triggers the AI agent to suggest corrections.
*   **Magic Input (`?`):** Start any command line with `?` to bypass the shell and ask the AI directly (e.g., `? how do I revert a git commit`).
*   **Human-in-the-Loop:** The AI proposes commands but *you* approve them. Dangerous commands are flagged for review.
*   **Streaming Responses:** Real-time AI feedback with markdown rendering and syntax highlighting.
*   **Cross-Platform:** Runs on Linux, macOS, and Windows.

## 🛠️ Architecture

The application is built on a modern Rust stack:

*   **GUI Framework:** `egui` + `eframe` (Immediate Mode GUI for high performance).
*   **Terminal Core:**
    *   `portable-pty`: Cross-platform pseudoterminal management.
    *   `vte`: Industry-standard ANSI escape sequence parser.
    *   `TerminalGrid`: Custom high-performance character grid with ring buffer scrolling.
*   **AI Orchestration:**
    *   **Async Event Loop:** Non-blocking background tasks handle LLM communication.
    *   **Streaming Client:** HTTP client supporting Server-Sent Events (SSE) for real-time interaction.
    *   **State Machine:** Robust handling of UI states (Normal, Waiting, Approval, Answer).

## 📦 Installation & Setup

### Prerequisites

*   Rust 1.75+
*   Linux dependencies: `libssl-dev`, `pkg-config`, `libxcb-shape0-dev`, `libxcb-xfixes0-dev`.

### Building from Source

```bash
# Clone the repository
git clone https://github.com/infraware/terminal.git
cd terminal-app

# Build and run
cargo run --release
```

### Configuration

The terminal looks for environment variables to connect to the backend:

```bash
export INFRAWARE_BACKEND_URL="http://your-backend-api.com"
export BACKEND_API_KEY="your-api-key"
```

If no API key is provided, the terminal falls back to a **Mock Client** for testing purposes.

## 💡 Usage Guide

### Standard Terminal
Use it just like `xterm` or `iTerm2`. Run commands, edit files, ssh into servers.

### Automatic Error Assistance
If you type a command that doesn't exist:
```bash
$ gti status
```
The terminal detects the error via a shell hook and asks the AI. The AI will suggest `git status`. You can approve execution with `Y`.

### Direct AI Query
Prefix your input with `?` to ask a question:
```bash
$ ? list all docker containers sorted by memory usage
```
The AI will generate the appropriate `docker stats` command format for you.

### Interactive Mode
If the AI needs more information (e.g., "Which region do you want to deploy to?"), it will pause and ask you. Type your answer directly in the prompt.

## 🤝 Contributing

We welcome contributions! Please follow our [Rust coding guidelines](CLAUDE.md) (Microsoft Pragmatic Rust).

1.  Fork the repo.
2.  Create a feature branch.
3.  Ensure `cargo check` and `cargo test` pass (zero warnings policy).
4.  Submit a Pull Request.

## 📜 License

MIT License. See [LICENSE](LICENSE) for details.