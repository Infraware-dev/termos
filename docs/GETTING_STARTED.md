# Getting Started

Guida per avviare Infraware Terminal con il backend Rust.

## Requisiti

- **Rust** 1.85+ (edition 2024)

```bash
# Dipendenze Linux
sudo apt install -y pkg-config libssl-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

**Opzionale** (solo per HttpEngine/ProcessEngine):
- Python 3.10+
- LangGraph

---

## Quick Start: RigEngine (Consigliato)

**RigEngine** è il motore primario raccomandato per Infraware Terminal. Utilizza rig-rs con API Anthropic Claude per un vero agente IA, senza dipendenze esterne (LangGraph non richiesto).

### Architettura (2 Terminali)

```
┌─────────────────┐    ┌─────────────────┐
│   Terminal 1    │    │   Terminal 2    │
│                 │    │                 │
│  Rust Backend   │◄───│  Terminal App   │
│  (RigEngine)    │    │    (egui)       │
│  (porta 8080)   │    │                 │
└────────┬────────┘    └─────────────────┘
         │
         ▼
   Anthropic API
   (Claude Sonnet)
```

### Prerequisiti

1. **API Key Anthropic** - Ottieni da [console.anthropic.com](https://console.anthropic.com/)

### Terminal 1: Backend RigEngine

```bash
cd /home/crist/infraware-terminal

# Imposta la API key Anthropic
export ANTHROPIC_API_KEY="sk-ant-api03-..."

# Avvia il backend con RigEngine
ENGINE_TYPE=rig cargo run -p infraware-backend --features rig
# Server parte su http://localhost:8080
```

### Terminal 2: Terminal App

```bash
cd /home/crist/infraware-terminal
cargo run -p infraware-terminal
# Si apre la finestra del terminale
```

### Test Interattivo

Nel terminale egui, digita:
```
? list files in current directory
```

Vedrai:
1. **AwaitingApproval**: "Execute: ls -la?" [y/n]
2. Premi **y** per approvare
3. Il comando esegue nel PTY
4. L'agente decide se risposta diretta o continuazione (`needs_continuation`)
5. **Normal**: Risultato visualizzato

### Come Funziona needs_continuation

Il flag `needs_continuation` determina il comportamento dopo l'esecuzione:

| Scenario | needs_continuation | Comportamento |
|----------|-------------------|---------------|
| "lista i file" → `ls -la` | `false` | Output del comando È la risposta |
| "installa redis" → `uname -s` | `true` | Output usato per decidere prossimo step |

Esempio flusso con `needs_continuation=true`:
```
User: "Come installo Redis?"
         ↓
Agent: execute_shell_command("uname -s", needs_continuation=true)
         ↓
[User approva] → Terminal esegue → "Linux"
         ↓
Agent riceve output, continua ragionamento:
"Su Linux, installa con: sudo apt-get install redis-server"
```

---

## MockEngine - Solo Testing

Il modo più veloce per testare il backend, senza API key o dipendenze esterne:

```bash
cd /home/crist/infraware-terminal

# MockEngine è il default
cargo run -p infraware-backend
# Server parte su http://localhost:8080
```

**Risposte pre-configurate:**
| Input contiene | Risposta |
|----------------|----------|
| `ls`, `list files` | Esempi comandi `ls` |
| `docker` | Esempi Docker |
| `kubernetes`, `k8s` | Esempi kubectl |
| `git` | Esempi Git |
| altro | Risposta generica |

### Test API con curl

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

## Engine Alternativi (LangGraph)

> **Nota**: Questi engine richiedono Python e LangGraph. Per la maggior parte degli utenti, **RigEngine è raccomandato**.

### HttpEngine - Proxy LangGraph

Setup a 3 terminali con proxy diretto al server LangGraph.

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Terminal 1    │    │   Terminal 2    │    │   Terminal 3    │
│                 │    │                 │    │                 │
│   LangGraph     │◄───│  Rust Backend   │◄───│  Terminal App   │
│   (porta 2024)  │    │  (porta 8080)   │    │    (egui)       │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

```bash
# Terminal 1: LangGraph Server
cd backend
pip install -r requirements.txt  # Prima volta
langgraph dev
# Server su http://localhost:2024

# Terminal 2: Backend Rust (HttpEngine)
ENGINE_TYPE=http \
LANGGRAPH_URL=http://localhost:2024 \
cargo run -p infraware-backend

# Terminal 3: Terminal App
cargo run -p infraware-terminal
```

### ProcessEngine - Bridge Python

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
cargo run -p infraware-backend
```

---

## Variabili d'Ambiente

```bash
# === Engine Selection ===
ENGINE_TYPE=mock|http|process|rig  # Default: mock

# === Server ===
PORT=8080                          # Default: 8080

# === RigEngine (Consigliato - Anthropic Claude API) ===
ANTHROPIC_API_KEY=sk-ant-api03-...  # Richiesta per ENGINE_TYPE=rig

# === LangGraph (per http/process engines) ===
LANGGRAPH_URL=http://localhost:2024

# === ProcessEngine ===
BRIDGE_COMMAND=python3
BRIDGE_SCRIPT=bin/engine-bridge/main.py
BRIDGE_WORKING_DIR=/path/to/dir   # Opzionale

# === Sicurezza ===
API_KEY=your-secret-key           # Vuoto = auth disabilitata
ALLOWED_ORIGINS=http://localhost:3000,http://localhost:8080
RATE_LIMIT_RPM=100                # 0 = disabilitato

# === Debug ===
RUST_LOG=infraware_backend=debug,tower_http=debug
```

**Quick Setup per RigEngine:**
```bash
# Opzione 1: Environment variable
export ANTHROPIC_API_KEY="sk-ant-api03-..."
ENGINE_TYPE=rig cargo run -p infraware-backend --features rig

# Opzione 2: .env file
echo "ANTHROPIC_API_KEY=sk-ant-api03-..." > .env
ENGINE_TYPE=rig cargo run -p infraware-backend --features rig
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

### RigEngine: "ANTHROPIC_API_KEY not set"

```bash
# Imposta la variabile d'ambiente
export ANTHROPIC_API_KEY="sk-ant-api03-..."

# Oppure crea file .env nella root
echo "ANTHROPIC_API_KEY=sk-ant-api03-..." > .env
```

### RigEngine: "invalid_api_key" o 401 da Anthropic

```bash
# Verifica che la chiave sia valida
curl -H "x-api-key: $ANTHROPIC_API_KEY" \
     -H "anthropic-version: 2023-06-01" \
     https://api.anthropic.com/v1/models
```

### "Unauthorized" (401) dal backend

```bash
# Se API_KEY è configurata, devi passare l'header:
curl -H "X-Api-Key: $API_KEY" http://localhost:8080/threads
```

### "Too Many Requests" (429)

```bash
# Rate limit superato. Attendi o disabilita:
RATE_LIMIT_RPM=0 cargo run -p infraware-backend
```

### LangGraph: "Connection refused" (solo HttpEngine/ProcessEngine)

```bash
# Verifica che LangGraph sia in esecuzione
curl http://localhost:2024/health

# Se non risponde, avvialo:
cd backend && langgraph dev
```

### ProcessEngine: Bridge Python non risponde

```bash
# Verifica dipendenze
pip install -r bin/engine-bridge/requirements.txt

# Test manuale del bridge
echo '{"jsonrpc":"2.0","id":"1","method":"health_check","params":{}}' | \
  python3 bin/engine-bridge/main.py
```

---

## Prossimi Passi

### Quick Start Path
1. **Test con MockEngine** - Familiarizza con l'API senza costi
   ```bash
   cargo run -p infraware-backend
   ```
2. **Setup RigEngine** - Usa agente nativo Rust (consigliato)
   ```bash
   export ANTHROPIC_API_KEY="sk-..."
   ENGINE_TYPE=rig cargo run -p infraware-backend --features rig
   ```
3. **Avvia Terminal** - Prova query in linguaggio naturale
   ```bash
   cargo run -p infraware-terminal
   # Type: ? list files
   ```

### Production Deployment
1. **Configura autenticazione** - Imposta `API_KEY` per proteggere il backend
2. **Monitora metriche** - Scrape `/metrics` endpoint con Prometheus
3. **Setup monitoring** - Configura alerting per Anthropic API rate limits
4. **Graceful shutdown** - Backend supporta SIGTERM e Ctrl+C

---

## Riferimenti

- [BACKEND_ARCHITECTURE.md](./BACKEND_ARCHITECTURE.md) - Architettura dettagliata
- [CODE_REVIEW_PLAN.md](./CODE_REVIEW_PLAN.md) - Piano di code review
- [OpenAPI Spec](http://localhost:8080/api-docs/openapi.json) - Documentazione API interattiva
