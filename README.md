# Infraware Terminal

Terminal emulator con assistente AI integrato per DevOps. Prefissa qualsiasi comando con `?` per query in linguaggio naturale.

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

| Engine | Uso | Comando |
|--------|-----|---------|
| **MockEngine** | Testing/sviluppo (default) | `cargo run -p infraware-backend` |
| **HttpEngine** | Produzione con LangGraph | `ENGINE_TYPE=http LANGGRAPH_URL=http://localhost:2024 cargo run -p infraware-backend` |
| **ProcessEngine** | Bridge Python | `ENGINE_TYPE=process BRIDGE_SCRIPT=bin/engine-bridge/main.py cargo run -p infraware-backend` |
| **RigEngine** | Agente Rust nativo con rig-rs | `ENGINE_TYPE=rig ANTHROPIC_API_KEY=sk-... cargo run -p infraware-backend --features rig` |

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
cargo fmt --all && cargo clippy --workspace

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

## License

MIT
