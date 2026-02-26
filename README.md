# Infraware Terminal

Terminal emulator con assistente AI integrato per DevOps. Prefissa qualsiasi comando con `?` per query in linguaggio
naturale.

## Requisiti

- **Rust** 1.85+ (edition 2024)
- **Linux**: dipendenze di sistema

```bash
# Ubuntu/Debian
sudo apt install -y pkg-config libssl-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

## Quick Start

```bash
# Clona e entra nella directory
cd infraware-terminal

# Avvia con MockEngine (nessuna dipendenza esterna)
ENGINE_TYPE=mock cargo run

# Oppure con RigEngine (default, richiede API key Anthropic)
ANTHROPIC_API_KEY=sk-... cargo run
```

### Usa l'assistente AI

Nel terminale, prefissa con `?` per query in linguaggio naturale:

```
? come faccio a vedere i container docker in esecuzione
? elenca i file nella directory corrente
? come faccio un revert dell'ultimo commit git
```

## Configurazione Engine

L'engine gira in-process nel terminale (nessun backend separato).

| Engine            | Uso                           | Comando                                            |
|-------------------|-------------------------------|----------------------------------------------------|
| **RigEngine**     | Agente Rust nativo (default)  | `ANTHROPIC_API_KEY=sk-... cargo run`               |
| **MockEngine**    | Testing/sviluppo              | `ENGINE_TYPE=mock cargo run`                       |

## Variabili d'Ambiente

```bash
# Engine
ENGINE_TYPE=rig|mock             # Default: rig
ANTHROPIC_API_KEY=sk-...         # Richiesta per RigEngine
ANTHROPIC_MODEL=claude-sonnet-4-20250514  # Opzionale, ha un default
RIG_MAX_TOKENS=4096              # Opzionale
RIG_TEMPERATURE=0.7              # Opzionale
RIG_TIMEOUT_SECS=300             # Opzionale

# Memory
MEMORY_PATH=./.infraware/memory.json  # Path sessione memoria
MEMORY_LIMIT=200                      # Max entries memoria

# MockEngine
MOCK_WORKFLOW_FILE=path/to/workflow.json  # Opzionale

# Logging
LOG_LEVEL=debug|info|warn|error  # Default: info
```

## Comandi Utili

```bash
# Build
cargo build

# Test
cargo test

# Lint
cargo +nightly fmt --all && cargo clippy

# Watch mode (ricompila automaticamente)
cargo watch -x run
```

## Struttura Progetto

```
infraware-terminal/
├── Cargo.toml                  # Single crate, no workspace
├── src/
│   ├── main.rs
│   ├── app.rs                  # Main InfrawareApp, eframe::App impl
│   ├── app/                    # Handler modules
│   │   ├── input_handler.rs    # Keyboard input
│   │   ├── hitl_handler.rs     # Human-in-the-loop
│   │   ├── llm_controller.rs   # Drives engine directly
│   │   ├── llm_event_handler.rs
│   │   ├── session_manager.rs
│   │   ├── tiles_manager.rs
│   │   └── ...
│   ├── engine/                 # Agentic engine (in-process)
│   │   ├── mod.rs
│   │   ├── traits.rs           # AgenticEngine, EventStream
│   │   ├── adapters/
│   │   │   ├── mock.rs         # MockEngine (testing)
│   │   │   └── rig/            # RigEngine (Anthropic Claude)
│   │   └── shared/             # Event types, models
│   ├── terminal/               # VTE parser, grid, cell
│   ├── pty/                    # PTY session, async I/O
│   ├── llm/                    # Markdown renderer
│   ├── input/                  # Keyboard mapping, command classification
│   ├── ui/                     # egui helpers, theme
│   └── config.rs
└── docs/
```

## Architettura

```
┌─────────────────────────────────────────────┐
│ infraware-terminal (single binary)          │
│                                             │
│  ┌───────────┐     ┌─────────────────────┐  │
│  │ Terminal   │     │ AgenticEngine       │  │
│  │ UI (egui) │◄───►│ (in-process)        │  │
│  └─────┬─────┘     │ ┌────────┐ ┌──────┐ │  │
│        │           │ │ Mock   │ │ Rig  │ │  │
│   ┌────▼────┐      │ │ Engine │ │Engine│ │  │
│   │   PTY   │      │ └────────┘ └──┬───┘ │  │
│   │ Session │      └───────────────┼─────┘  │
│   └────┬────┘                      │        │
│   ┌────▼────┐               ┌──────▼──────┐ │
│   │  VTE    │               │ Anthropic   │ │
│   │ Parser  │               │ API         │ │
│   └─────────┘               └─────────────┘ │
└─────────────────────────────────────────────┘
```

I comandi vengono eseguiti tramite il PTY del terminale, non internamente dall'engine.

## Keyboard Shortcuts

| Shortcut                       | Action             | Platform      |
|--------------------------------|--------------------|---------------|
| `Cmd+T` / `Ctrl+Shift+T`       | New tab            | macOS / Linux |
| `Cmd+W` / `Ctrl+Shift+W`       | Close tab          | macOS / Linux |
| `Ctrl+Tab`                     | Next tab           | All           |
| `Ctrl+Shift+Tab`               | Previous tab       | All           |
| `Cmd+Shift+H` / `Ctrl+Shift+H` | Split horizontal   | macOS / Linux |
| `Cmd+Shift+J` / `Ctrl+Shift+J` | Split vertical     | macOS / Linux |
| `Cmd+C` / `Ctrl+Shift+C`       | Copy               | macOS / Linux |
| `Cmd+V` / `Ctrl+Shift+V`       | Paste              | macOS / Linux |
| `Ctrl+C`                       | SIGINT (interrupt) | All           |
| `Ctrl+D`                       | EOF                | All           |
| `Ctrl+L`                       | Clear screen       | All           |
| `Ctrl+Shift+/`                 | Enter LLM mode     | All           |

## Mock Workflow File

This file lets you create a playbook for the MockEngine.

### Workflow JSON Schema

The workflow file defines the scripted investigation the agent follows.

```json
{
  "run_commands": true,
  "playbooks": {
    "my-playbook": {
      "name": "My Playbook",
      "intents": [
        "can you investigate docker issues",
        "could you troubleshoot container problems"
      ],
      "phases": [
        {
          "phase": 1,
          "name": "Phase Name",
          "description": "What this phase accomplishes",
          "duration_minutes": 5,
          "steps": [
            {
              "step": 1,
              "action": "Human-readable description of what agent is doing",
              "command": "shell command to execute",
              "output": "expected output (used for static replay or validation)",
              "analysis": "Agent's interpretation of the results"
            }
          ],
          "conclusion": "Summary at end of phase (optional)"
        }
      ]
    }
  }
}
```

#### Root Workflow Fields

| Field          | Type                    | Required | Description                                                      |
|----------------|-------------------------|----------|------------------------------------------------------------------|
| `run_commands` | `bool`                  | Yes      | Whether to actually run commands or just return the output field |
| `playbooks`    | `Map<String, Playbook>` | Yes      | Collection of playbooks identified by unique keys                |

#### Playbook Object

| Field     | Type          | Required | Description                                 |
|-----------|---------------|----------|---------------------------------------------|
| `name`    | `String`      | Yes      | Display name (e.g., "Docker Investigation") |
| `intents` | `Vec<String>` | Yes      | List of intents the playbook addresses      |
| `phases`  | `Vec<Phase>`  | Yes      | Ordered list of phases                      |

#### Phase Object

| Field                  | Type                  | Required | Description                                            |
|------------------------|-----------------------|----------|--------------------------------------------------------|
| `phase`                | `u32`                 | Yes      | 1-indexed phase number                                 |
| `name`                 | `String`              | Yes      | Display name (e.g., "Symptom Verification")            |
| `description`          | `String`              | Yes      | What this phase accomplishes                           |
| `duration_minutes`     | `u32`                 | No       | Estimated duration for display                         |
| `steps`                | `Vec<Step>`           | No       | Steps to execute (absent in documentation-only phases) |
| `conclusion`           | `String`              | No       | Summary statement at phase end                         |
| `root_cause`           | `RootCause`           | No       | Present only in root cause phase                       |
| `verification_summary` | `Map<String, String>` | No       | Present only in verification phase                     |

#### Step Object

| Field      | Type     | Required | Description                                  |
|------------|----------|----------|----------------------------------------------|
| `step`     | `u32`    | Yes      | Global step number (continues across phases) |
| `action`   | `String` | Yes      | What the agent is about to do                |
| `command`  | `String` | Yes      | Shell command to execute/mock                |
| `output`   | `String` | Yes      | Expected output or recorded output           |
| `analysis` | `String` | Yes      | Agent's reasoning about the result           |

#### RootCause Object

| Field        | Type     | Description                                   |
|--------------|----------|-----------------------------------------------|
| `issue`      | `String` | Technical description of the problem          |
| `impact`     | `String` | User-facing impact                            |
| `drift_type` | `String` | Classification (e.g., "Infrastructure drift") |

### Run with a Workflow file

```bash
MOCK_WORKFLOW_FILE=~/Downloads/workflow.json ENGINE_TYPE=mock cargo run
```

## License

MIT
