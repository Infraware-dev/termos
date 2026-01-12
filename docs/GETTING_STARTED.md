# Getting Started

Guida rapida per avviare il backend Infraware e testare con il MockEngine.

## Requisiti

- **Rust** 1.85+ (edition 2024)
- **Python 3.10+** (solo per ProcessEngine)
- **LangGraph** (solo per HttpEngine/ProcessEngine)

---

## Quick Start (MockEngine)

Il modo più veloce per testare il backend, senza dipendenze esterne:

```bash
cd /home/crist/infraware-terminal

# Avvia il server (MockEngine è il default)
cargo run --bin infraware-backend
```

Il server parte su `http://localhost:8080`.

### Test rapido

```bash
# Health check
curl http://localhost:8080/health

# Crea un thread
curl -X POST http://localhost:8080/threads \
  -H "Content-Type: application/json" \
  -d '{}'

# Stream una risposta (SSE)
curl -N -X POST http://localhost:8080/threads/mock-thread-1/runs/stream \
  -H "Content-Type: application/json" \
  -d '{
    "assistant_id": "agent",
    "input": {
      "messages": [{"role": "user", "content": "list files"}]
    }
  }'
```

---

## Configurazioni Engine

### 1. MockEngine (Default) - Testing

Risposte simulate in-memory. Ideale per sviluppo UI e test.

```bash
ENGINE_TYPE=mock cargo run --bin infraware-backend
```

**Risposte pre-configurate:**
| Input contiene | Risposta |
|----------------|----------|
| `ls`, `list files` | Esempi comandi `ls` |
| `docker` | Esempi Docker |
| `kubernetes`, `k8s` | Esempi kubectl |
| `git` | Esempi Git |
| altro | Risposta generica |

### 2. HttpEngine - Produzione con LangGraph

Proxy diretto al server LangGraph.

```bash
# Terminal 1: Avvia LangGraph
cd backend
langgraph dev  # Porta 2024

# Terminal 2: Avvia backend Rust
ENGINE_TYPE=http \
LANGGRAPH_URL=http://localhost:2024 \
cargo run --bin infraware-backend
```

### 3. ProcessEngine - Bridge Python

Comunicazione via subprocess (JSON-RPC over stdio).

```bash
# Installa dipendenze bridge
pip install -r bin/engine-bridge/requirements.txt

# Avvia LangGraph
cd backend && langgraph dev &

# Avvia backend con ProcessEngine
ENGINE_TYPE=process \
BRIDGE_SCRIPT=bin/engine-bridge/main.py \
LANGGRAPH_URL=http://localhost:2024 \
cargo run --bin infraware-backend
```

### 4. RigEngine - Agente Rust Nativo

Agente LLM implementato nativamente in Rust usando [rig-rs](https://github.com/0xPlaygrounds/rig). Chiama direttamente l'API Anthropic senza bisogno di LangGraph.

**Setup API Key:**

```bash
cd crates/backend-api

# Copia il template dei secrets
cp .env.secrets.example .env.secrets

# Edita .env.secrets e inserisci la tua API key Anthropic
# ANTHROPIC_API_KEY=sk-ant-api03-...
```

**Avvia il backend:**

```bash
ENGINE_TYPE=rig cargo run --bin infraware-backend
```

**File di configurazione:**
```
crates/backend-api/
├── .env                 # Config generale (PORT, ENGINE_TYPE, etc.)
├── .env.secrets         # Secrets (ANTHROPIC_API_KEY) - in .gitignore
├── .env.example         # Template per .env
└── .env.secrets.example # Template per .env.secrets
```

> **Nota:** `.env.secrets` non viene committato nel repository. Assicurati di crearlo manualmente.

---

## Variabili d'Ambiente

```bash
# === Engine ===
ENGINE_TYPE=mock|http|process|rig  # Default: mock

# === Server ===
PORT=8080                          # Default: 8080

# === LangGraph (per http/process) ===
LANGGRAPH_URL=http://localhost:2024

# === ProcessEngine ===
BRIDGE_COMMAND=python3
BRIDGE_SCRIPT=bin/engine-bridge/main.py
BRIDGE_WORKING_DIR=/path/to/dir   # Opzionale

# === RigEngine (in .env.secrets) ===
ANTHROPIC_API_KEY=sk-ant-api03-...  # Richiesta per ENGINE_TYPE=rig

# === Sicurezza ===
API_KEY=your-secret-key           # Vuoto = auth disabilitata
ALLOWED_ORIGINS=http://localhost:3000,http://localhost:8080
RATE_LIMIT_RPM=100                # 0 = disabilitato

# === Debug ===
RUST_LOG=infraware_backend=debug,tower_http=debug
```

---

## Endpoints API

| Metodo | Path | Descrizione | Auth |
|--------|------|-------------|------|
| `GET` | `/health` | Health check | No |
| `GET` | `/metrics` | Prometheus metrics | No |
| `GET` | `/api-docs/openapi.json` | OpenAPI spec | No |
| `POST` | `/api/auth` | Verifica autenticazione | No |
| `POST` | `/threads` | Crea nuovo thread | Sì* |
| `POST` | `/threads/{id}/runs/stream` | Avvia/riprende run (SSE) | Sì* |

*Auth richiesta solo se `API_KEY` è configurata.

### Autenticazione

Se `API_KEY` è impostata, usa uno di questi header:

```bash
# Bearer token
curl -H "Authorization: Bearer your-api-key" ...

# X-Api-Key header
curl -H "X-Api-Key: your-api-key" ...
```

---

## Esempi Completi

### Esempio 1: Conversazione semplice

```bash
# 1. Crea thread
THREAD_ID=$(curl -s -X POST http://localhost:8080/threads \
  -H "Content-Type: application/json" \
  -d '{}' | jq -r '.thread_id')

echo "Thread creato: $THREAD_ID"

# 2. Invia messaggio e ricevi stream SSE
curl -N -X POST "http://localhost:8080/threads/$THREAD_ID/runs/stream" \
  -H "Content-Type: application/json" \
  -d '{
    "assistant_id": "agent",
    "input": {
      "messages": [{"role": "user", "content": "Come posso vedere i container Docker?"}]
    }
  }'
```

### Esempio 2: HITL (Human-in-the-Loop)

Quando l'agente richiede approvazione per un comando:

```bash
# Il server invia un evento SSE con interrupt:
# event: updates
# data: {"__interrupt__":[{"value":{"command":"docker ps","message":"Vuoi eseguire?"}}]}

# Per approvare, riprendi il run:
curl -N -X POST "http://localhost:8080/threads/$THREAD_ID/runs/stream" \
  -H "Content-Type: application/json" \
  -d '{
    "assistant_id": "agent",
    "command": {"resume": "approved"}
  }'

# Per rifiutare:
curl -N -X POST "http://localhost:8080/threads/$THREAD_ID/runs/stream" \
  -H "Content-Type: application/json" \
  -d '{
    "assistant_id": "agent",
    "command": {"resume": "rejected"}
  }'
```

### Esempio 3: Rispondere a una domanda

Quando l'agente fa una domanda:

```bash
# Evento SSE con question interrupt:
# event: updates
# data: {"__interrupt__":[{"value":{"question":"Quale ambiente?","options":["dev","prod"]}}]}

# Rispondi con input:
curl -N -X POST "http://localhost:8080/threads/$THREAD_ID/runs/stream" \
  -H "Content-Type: application/json" \
  -d '{
    "assistant_id": "agent",
    "command": {"resume": "approved"},
    "input": {
      "messages": [{"role": "user", "content": "prod"}]
    }
  }'
```

---

## Sviluppo

### Build e Test

```bash
# Build completo
cargo build --workspace

# Test completi
cargo test --workspace

# Solo backend
cargo test -p infraware-backend
cargo test -p infraware-engine

# Con output verboso
RUST_LOG=debug cargo test --workspace -- --nocapture
```

### Watch mode (ricompila automaticamente)

```bash
# Installa cargo-watch
cargo install cargo-watch

# Riavvia server ad ogni modifica
cargo watch -x 'run --bin infraware-backend'
```

### Logs strutturati

```bash
# Debug completo
RUST_LOG=debug cargo run --bin infraware-backend

# Solo errori
RUST_LOG=warn cargo run --bin infraware-backend

# Filtro specifico
RUST_LOG=infraware_backend::routes=debug cargo run --bin infraware-backend
```

---

## Troubleshooting

### "Connection refused" su LangGraph

```bash
# Verifica che LangGraph sia in esecuzione
curl http://localhost:2024/health

# Se non risponde, avvialo:
cd backend && langgraph dev
```

### "Unauthorized" (401)

```bash
# Se API_KEY è configurata, devi passare l'header:
curl -H "X-Api-Key: $API_KEY" http://localhost:8080/threads
```

### "Too Many Requests" (429)

```bash
# Rate limit superato. Attendi o disabilita:
RATE_LIMIT_RPM=0 cargo run --bin infraware-backend
```

### Bridge Python non risponde

```bash
# Verifica dipendenze
pip install -r bin/engine-bridge/requirements.txt

# Test manuale del bridge
echo '{"jsonrpc":"2.0","id":"1","method":"health_check","params":{}}' | \
  python3 bin/engine-bridge/main.py
```

---

## Prossimi Passi

1. **Test con MockEngine** - Familiarizza con l'API
2. **Setup LangGraph** - Per risposte reali da LLM
3. **Configura auth** - Imposta `API_KEY` per produzione
4. **Monitora metriche** - Scrape `/metrics` con Prometheus

---

## Riferimenti

- [BACKEND_ARCHITECTURE.md](./BACKEND_ARCHITECTURE.md) - Architettura dettagliata
- [CODE_REVIEW_PLAN.md](./CODE_REVIEW_PLAN.md) - Piano di code review
- [OpenAPI Spec](http://localhost:8080/api-docs/openapi.json) - Documentazione API interattiva
