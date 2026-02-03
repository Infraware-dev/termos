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

### 1. Avvia il Backend

```bash
# Clona e entra nella directory
cd infraware-terminal

# Avvia il backend con MockEngine (nessuna dipendenza esterna)
cargo run -p infraware-backend
```

Il server parte su `http://localhost:8080`. Verifica con:

```bash
curl http://localhost:8080/health
# {"status":"healthy","engine":"mock",...}
```

### 2. Avvia il Terminal

In un altro terminale:

```bash
cargo run -p infraware-terminal
```

### 3. Usa l'assistente AI

Nel terminale, prefissa con `?` per query in linguaggio naturale:

```
? come faccio a vedere i container docker in esecuzione
? elenca i file nella directory corrente
? come faccio un revert dell'ultimo commit git
```

## Configurazione Engine

| Engine            | Uso                           | Comando                                                                                      |
|-------------------|-------------------------------|----------------------------------------------------------------------------------------------|
| **MockEngine**    | Testing/sviluppo (default)    | `cargo run -p infraware-backend`                                                             |
| **HttpEngine**    | Produzione con LangGraph      | `ENGINE_TYPE=http LANGGRAPH_URL=http://localhost:2024 cargo run -p infraware-backend`        |
| **ProcessEngine** | Bridge Python                 | `ENGINE_TYPE=process BRIDGE_SCRIPT=bin/engine-bridge/main.py cargo run -p infraware-backend` |
| **RigEngine**     | Agente Rust nativo con rig-rs | `ENGINE_TYPE=rig ANTHROPIC_API_KEY=sk-... cargo run -p infraware-backend --features rig`     |

### Produzione con LangGraph

```bash
# Terminal 1: Avvia LangGraph
cd backend
langgraph dev

# Terminal 2: Avvia backend Rust
ENGINE_TYPE=http LANGGRAPH_URL=http://localhost:2024 cargo run -p infraware-backend

# Terminal 3: Avvia il terminal
cargo run -p infraware-terminal
```

## Variabili d'Ambiente

```bash
# Backend
ENGINE_TYPE=mock|http|process    # Default: mock
PORT=8080                        # Default: 8080
API_KEY=your-secret-key          # Vuoto = auth disabilitata
RATE_LIMIT_RPM=100               # 0 = disabilitato
MOCK_WORKFLOW_FILE=path/to/workflow.json  # MockEngine only

# LangGraph (per http/process engine)
LANGGRAPH_URL=http://localhost:2024

# Terminal
INFRAWARE_BACKEND_URL=http://localhost:8080
LOG_LEVEL=debug|info|warn|error  # Default: info
```

## Comandi Utili

```bash
# Build tutto
cargo build --workspace

# Test
cargo test --workspace

# Lint
cargo +nightly fmt --all && cargo clippy --workspace

# Watch mode (ricompila automaticamente)
cargo watch -x 'run -p infraware-backend'
```

## Struttura Progetto

```
infraware-terminal/
├── terminal-app/           # Client terminal (egui)
├── crates/
│   ├── backend-api/        # Server REST/SSE (axum)
│   ├── backend-engine/     # Engine trait + adapters
│   ├── backend-state/      # State persistence
│   └── shared/             # Tipi condivisi
├── backend/                # Python FastAPI (legacy)
└── bin/engine-bridge/      # Bridge Python per ProcessEngine
```

## Documentazione

- [Getting Started](docs/GETTING_STARTED.md) - Guida dettagliata
- [Backend Architecture](docs/BACKEND_ARCHITECTURE.md) - Architettura backend
- [OpenAPI Spec](http://localhost:8080/api-docs/openapi.json) - API docs (quando il server e' attivo)

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

> Remember that `intents` must start with the words specified at `terminal-app/config/language.toml`, such as "can
> you", "could you", etc.

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

### Run the Backend with a Workflow file

In order to run the `MockEngine` with a custom workflow file, set the
`MOCK_WORKFLOW_FILE` environment variable to point to the JSON file and the `ENGINE_TYPE` to `mock`:

```bash
MOCK_WORKFLOW_FILE=~/Downloads/workflow.json ENGINE_TYPE=mock cargo run -p infraware-backend
```

Then use the terminal setting a mocked API key and backend URL:

```bash
ANTHROPIC_API_KEY="abcdef" INFRAWARE_BACKEND_URL="http://localhost:8080" cargo run -p infraware-terminal
```

## License

MIT
