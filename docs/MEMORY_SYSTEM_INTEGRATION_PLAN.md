# Memory System Integration Plan

**Data**: 2026-02-06
**Autore**: Analisi di integrazione sistema di memoria nel RigEngine
**Riferimento POC**: `/home/crist/memory_test`

---

## Executive Summary

Questo documento analizza l'integrazione di un sistema di memoria semantica nel **RigEngine** di Infraware Terminal, basandosi sul POC esistente in `memory_test`. Il sistema permetterebbe al terminal AI di ricordare e imparare dai comandi passati, migliorando significativamente l'esperienza utente attraverso pre-retrieval semantico e context-aware suggestions.

**Conclusione**: ✅ **Fattibile e fortemente raccomandato** con approccio incrementale (MVP → Semantic → Advanced)

---

## 1. Architettura Attuale del RigEngine

### 1.1 Struttura Esistente

```
RigEngine (infraware-terminal/crates/backend-engine)
├─ AgenticEngine trait (astrazione)
├─ StateStore (in-memory)
│  └─ HashMap<ThreadId, ThreadState>
│     ├─ messages: Vec<Message>     ← Storia conversazione
│     └─ active_run: Option<RunState>
├─ Orchestrator (create_run_stream)
│  ├─ multi_turn(1) execution
│  ├─ PromptHook per HITL
│  └─ Tool calling (shell, ask_user)
└─ NO persistence, NO semantic search
```

### 1.2 Caratteristiche Chiave

- **Thread-based conversations**: ogni thread mantiene il proprio history
- **In-memory state**: `StateStore` con `RwLock<HashMap>`
- **History NON usata nelle nuove query** (orchestrator.rs:199-202):
  ```rust
  // NOTE: We intentionally DON'T use chat_history for new queries.
  // Each query is independent - history was causing LLM confusion
  ```
- **HITL pattern**: interrupt → user response → resume
- **Stateless tra restart**: nessuna persistenza su disco

### 1.3 Problema Attuale

La history lineare causa confusione all'LLM quando cresce. La soluzione attuale è **non usarla**, ma questo significa:
- ❌ Nessuna memoria cross-session
- ❌ L'agent riparte da zero ogni volta
- ❌ Nessun apprendimento dal comportamento utente

---

## 2. Sistema di Memoria Proposto (da memory_test POC)

### 2.1 Architettura Dual Storage

```
Memory System
├─ JSONL Storage (data/interactions.jsonl)
│  ├─ Append-only complete records
│  ├─ Human-readable
│  └─ Single source of truth
│
├─ LanceDB Vector Store (data/lancedb/)
│  ├─ Embeddings per semantic search
│  ├─ Metadata: ID, intent, timestamp, working_dir, data_type
│  └─ Ottimizzato per <100k records
│
└─ Intent Generation
   ├─ Commands: Claude API → semantic description
   ├─ Natural Language: regex normalization
   └─ Caching per ridurre API costs
```

### 2.2 Data Flow

```
┌─────────────────────────────────────────────────────────┐
│ 1. STORAGE                                              │
│ Command: "docker ps"                                    │
│    ↓                                                    │
│ Intent Generation (Claude API)                          │
│    ↓                                                    │
│ Intent: "list running containers"                       │
│    ↓                                                    │
│ Embedding (sentence-transformers)                       │
│    ↓                                                    │
│ Store: JSONL + LanceDB                                  │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│ 2. RETRIEVAL (Pre-retrieval)                           │
│ User Query: "mostra i container attivi"                 │
│    ↓                                                    │
│ Query Embedding                                         │
│    ↓                                                    │
│ LanceDB Vector Search (top-k=3)                         │
│    ↓                                                    │
│ Retrieve IDs → Fetch from JSONL                         │
│    ↓                                                    │
│ Format Context for Agent                                │
│    ↓                                                    │
│ Inject in System Prompt                                 │
│    ↓                                                    │
│ LLM Call with Memory Context                            │
└─────────────────────────────────────────────────────────┘
```

### 2.3 Schema Dati (InteractionRecord)

```json
{
  "id": "uuid",
  "timestamp": "ISO-8601",
  "data_type": "command" | "natural_language",
  "intent": "semantic description of what it does/wants",
  "input": "original command or query",
  "stderr": false,
  "context": {
    "working_dir": "/path/to/project"
  }
}
```

**Nota critica**: Nessun campo `output`. L'intent è l'interpretazione semantica, non il risultato dell'esecuzione.

---

## 3. Confronto Sistemi

| Aspetto | RigEngine (attuale) | Memory Test POC | RigEngine (enhanced) |
|---------|---------------------|-----------------|---------------------|
| **Storage** | In-memory HashMap | JSONL + LanceDB | StateStore + MemoryStore |
| **Persistence** | ❌ No | ✅ Sì | ✅ Sì |
| **Semantic Search** | ❌ No | ✅ Sì (vector embeddings) | ✅ Sì |
| **Intent Generation** | ❌ No | ✅ Sì (Claude API) | ✅ Sì |
| **Pre-retrieval** | ❌ No | ✅ Sì (top-k prima di LLM) | ✅ Sì |
| **Working Dir Filter** | ❌ No | ✅ Sì | ✅ Sì |
| **Cross-session Memory** | ❌ No | ✅ Sì | ✅ Sì |
| **Tool per search** | ❌ No | ✅ Sì (MemorySearchTool) | ✅ Sì |
| **Thread State** | ✅ Sì | ❌ No | ✅ Sì (mantiene entrambi) |

---

## 4. Piano di Integrazione

### 4.1 Architettura Enhanced

```rust
// Nuovo modulo: crates/backend-engine/src/adapters/rig/memory/

RigEngine (enhanced)
├─ StateStore (mantieni per stato runtime)
│  ├─ ThreadState → pending interrupts + session state
│  └─ memory_store: Option<Arc<MemoryStore>>  ← NEW
│
└─ MemoryStore (nuovo)
   ├─ Storage Backend
   │  ├─ JSONL writer/reader
   │  └─ LanceDB client
   ├─ Embedding Engine
   │  ├─ Opzione 1: Python subprocess (sentence-transformers)
   │  ├─ Opzione 2: Pure Rust (candle + ONNX)
   │  └─ Opzione 3: HTTP service
   ├─ Intent Generator
   │  ├─ Uses existing Anthropic client
   │  └─ In-memory caching
   └─ Retriever
      ├─ Vector search
      ├─ Working dir filtering
      └─ Context formatting

Tools (enhanced)
├─ ShellCommandTool (existing)
├─ AskUserTool (existing)
└─ MemorySearchTool (nuovo)
   └─ Semantic search durante reasoning
```

### 4.2 Punti di Integrazione

#### A. Pre-Retrieval in `create_run_stream()`

**File**: `src/adapters/rig/orchestrator.rs`, linea ~153

```rust
pub fn create_run_stream(
    config: Arc<RigEngineConfig>,
    client: Arc<anthropic::Client>,
    state: Arc<StateStore>,
    thread_id: infraware_shared::ThreadId,
    input: RunInput,
    run_id: String,
) -> EventStream {
    Box::pin(stream! {
        yield Ok(AgentEvent::metadata(&run_id));

        // Extract prompt
        let prompt = input.messages.iter()
            .filter(|m| m.role == MessageRole::User)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        // ✨ NEW: Pre-retrieval before agent execution
        let memory_context = if let Some(ref memory_store) = state.memory_store {
            let working_dir = extract_working_dir(&input);

            match memory_store.search_similar(&prompt, 3, working_dir).await {
                Ok(results) => format_memory_context_for_agent(&results),
                Err(e) => {
                    tracing::warn!("Memory retrieval failed: {:?}", e);
                    String::new()
                }
            }
        } else {
            String::new()
        };

        // Create agent with memory-enhanced preamble
        let agent = if !memory_context.is_empty() {
            let enhanced_preamble = format!(
                "{}\n\n## RELEVANT PAST INTERACTIONS\n{}",
                config.system_prompt,
                memory_context
            );

            client.agent(&config.model)
                .preamble(&enhanced_preamble)
                .max_tokens(config.max_tokens as u64)
                .temperature(f64::from(config.temperature))
                .tool(ShellCommandTool::new())
                .tool(AskUserTool::new())
                .tool(MemorySearchTool::new(&state))  // ← NEW TOOL
                .build()
        } else {
            create_agent(&client, &config)
        };

        // ... resto del codice esistente
    })
}
```

#### B. Storage delle Interazioni dopo Esecuzione

**File**: `src/adapters/rig/orchestrator.rs`, funzione `create_resume_stream()`

```rust
// Dopo CommandExecutionResult::Completed
match execution_result {
    CommandExecutionResult::Completed(output) => {
        // Show command output
        let response_text = format!("```\n$ {}\n{}\n```", command, output.trim());
        yield Ok(AgentEvent::Message(MessageEvent::assistant(&response_text)));

        // ✨ NEW: Store interaction in memory
        if let Some(ref memory_store) = state.memory_store {
            let working_dir = std::env::current_dir()
                .ok()
                .and_then(|p| p.to_str().map(|s| s.to_string()));

            let record = InteractionRecord {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                data_type: "command".to_string(),
                intent: memory_store.generate_intent(command, "command").await
                    .unwrap_or_else(|_| format!("executed {}", command)),
                input: command.clone(),
                stderr: !output.contains("successfully") && output.contains("error"),
                context: {
                    let mut ctx = std::collections::HashMap::new();
                    if let Some(wd) = working_dir {
                        ctx.insert("working_dir".to_string(), serde_json::Value::String(wd));
                    }
                    ctx
                },
            };

            if let Err(e) = memory_store.add_interaction(record).await {
                tracing::warn!("Failed to store interaction in memory: {:?}", e);
            }
        }

        // ... resto del codice esistente
    }
}
```

#### C. Nuovo Tool: MemorySearchTool

**File**: `src/adapters/rig/tools/memory.rs` (nuovo)

```rust
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemorySearchArgs {
    /// The search query to find similar past interactions
    query: String,
    /// Number of results to return (default: 3)
    #[serde(default = "default_top_k")]
    top_k: usize,
}

fn default_top_k() -> usize { 3 }

#[derive(Debug, Serialize, Deserialize)]
pub enum MemorySearchResult {
    Success { matches: Vec<String> },
    NoMatches,
    Error { message: String },
}

pub struct MemorySearchTool {
    state: Arc<StateStore>,
}

impl MemorySearchTool {
    pub fn new(state: &Arc<StateStore>) -> Self {
        Self {
            state: Arc::clone(state),
        }
    }
}

#[async_trait::async_trait]
impl Tool for MemorySearchTool {
    const NAME: &'static str = "search_memory";

    type Args = MemorySearchArgs;
    type Output = MemorySearchResult;
    type Error = anyhow::Error;

    async fn definition(&self, _prompt: String) -> rig::tool::ToolDefinition {
        rig::tool::ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search your memory for similar past interactions, commands, or queries. Use this to recall what you've done before in similar situations.".to_string(),
            parameters: schemars::schema_for!(Self::Args),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        if let Some(ref memory_store) = self.state.memory_store {
            match memory_store.search_similar(&args.query, args.top_k, None).await {
                Ok(results) if !results.is_empty() => {
                    let matches = results.iter()
                        .map(|r| format!("- {}: {}", r.record.input, r.record.intent))
                        .collect();

                    Ok(MemorySearchResult::Success { matches })
                }
                Ok(_) => Ok(MemorySearchResult::NoMatches),
                Err(e) => Ok(MemorySearchResult::Error {
                    message: format!("Memory search failed: {}", e)
                }),
            }
        } else {
            Ok(MemorySearchResult::Error {
                message: "Memory system not initialized".to_string()
            })
        }
    }
}
```

### 4.3 Moduli da Creare

```
crates/backend-engine/src/adapters/rig/memory/
├─ mod.rs              // Public API
├─ store.rs            // MemoryStore implementation
├─ models.rs           // InteractionRecord, SearchResult
├─ embeddings.rs       // Embedding engine (Python/Rust)
├─ intent.rs           // Intent generator
├─ retrieval.rs        // Context formatting
└─ jsonl.rs           // JSONL reader/writer utilities
```

### 4.4 Dipendenze Cargo.toml

```toml
[dependencies]
# Existing...

# Memory feature dependencies
lancedb = { version = "0.15", optional = true }
chrono = { version = "0.4", optional = true }

# Embeddings - Opzione 1: Python
pyo3 = { version = "0.23", features = ["auto-initialize"], optional = true }

# Embeddings - Opzione 2: Pure Rust (più complesso)
candle-core = { version = "0.8", optional = true }
candle-nn = { version = "0.8", optional = true }
tokenizers = { version = "0.20", optional = true }

[features]
# Existing rig feature
rig = ["rig-core", "async-stream", "regex", "schemars"]

# New memory feature (Python embeddings - recommended for MVP)
memory = ["lancedb", "chrono", "pyo3"]

# Advanced: pure Rust embeddings
memory-rust = ["lancedb", "chrono", "candle-core", "candle-nn", "tokenizers"]
```

---

## 5. Approccio Incrementale (Raccomandato)

### 5.1 Fase 1: MVP (2-3 giorni) ⭐ **START HERE**

**Obiettivo**: Persistenza base senza semantic search

**Componenti**:
- ✅ JSONL storage (append-only)
- ✅ Simple intent extraction (regex-based, no API)
- ✅ Linear search in memoria (ok fino ~1000 records)
- ✅ Storage solo comandi eseguiti
- ✅ Pre-retrieval con text matching semplice

**Benefici**:
- Persistenza cross-session
- Zero dipendenze esterne
- Nessun API cost
- 80% del valore con 20% della complessità

**Implementazione**:
```rust
// Simple in-memory index after loading from JSONL
struct SimpleMemoryStore {
    interactions: Vec<InteractionRecord>,
    jsonl_path: PathBuf,
}

impl SimpleMemoryStore {
    fn search_similar(&self, query: &str, top_k: usize) -> Vec<&InteractionRecord> {
        // Simple text matching - no embeddings yet
        let query_lower = query.to_lowercase();

        let mut matches: Vec<_> = self.interactions.iter()
            .filter(|r| {
                r.input.to_lowercase().contains(&query_lower) ||
                r.intent.to_lowercase().contains(&query_lower)
            })
            .collect();

        matches.truncate(top_k);
        matches
    }
}
```

**Test di validazione**:
```bash
# 1. Esegui comandi
? list docker containers
→ docker ps

# 2. Restart backend
cargo run -p infraware-backend

# 3. Query simile
? show running containers
→ Pre-retrieval trova "docker ps" dalla storia
→ Suggerisce basandosi su memoria
```

### 5.2 Fase 2: Semantic Search (5-7 giorni)

**Obiettivo**: Vector embeddings per similarity intelligente

**Componenti**:
- ✅ LanceDB integration
- ✅ Embeddings via Python subprocess (sentence-transformers)
- ✅ Pre-retrieval con cosine similarity
- ✅ Working directory filtering
- ✅ Intent generation con Claude API (+ caching aggressivo)

**Benefici**:
- Matching semantico (non solo keyword)
- "show containers" trova "docker ps" anche senza parole comuni
- Context-aware per progetto

**Test di validazione**:
```bash
# In /home/user/project-a
? list services
→ Trova "docker-compose ps" (working_dir=/home/user/project-a)

# In /home/user/project-b
? list services
→ Trova "kubectl get pods" (working_dir=/home/user/project-b)
```

### 5.3 Fase 3: Advanced (opzionale)

**Obiettivo**: Ottimizzazioni e features avanzate

**Componenti**:
- ✅ MemorySearchTool per agent (ricerca durante reasoning)
- ✅ Pure Rust embeddings (candle) per eliminare Python
- ✅ Temporal decay (score più alto per interazioni recenti)
- ✅ Clustering/deduplication
- ✅ Feedback loop (thumbs up/down su suggestions)
- ✅ Memory analytics dashboard

---

## 6. Stima Effort e Risorse

### 6.1 Breakdown per Fase

#### Fase 1: MVP
| Componente | Complessità | Tempo |
|------------|-------------|-------|
| JSONL storage | 🟢 Bassa | 4h |
| Simple search | 🟢 Bassa | 4h |
| Pre-retrieval hook | 🟢 Bassa | 4h |
| Storage integration | 🟢 Bassa | 4h |
| Tests | 🟢 Bassa | 4h |
| **TOTALE FASE 1** | - | **2-3 giorni** |

#### Fase 2: Semantic Search
| Componente | Complessità | Tempo |
|------------|-------------|-------|
| LanceDB integration | 🟡 Media | 1-2 giorni |
| Embeddings (Python) | 🟡 Media | 1-2 giorni |
| Intent generation | 🟡 Media | 1 giorno |
| Vector search | 🟡 Media | 1 giorno |
| Working dir filter | 🟢 Bassa | 4h |
| Tests | 🟡 Media | 1 giorno |
| **TOTALE FASE 2** | - | **5-7 giorni** |

#### Fase 3: Advanced (opzionale)
| Componente | Complessità | Tempo |
|------------|-------------|-------|
| MemorySearchTool | 🟢 Bassa | 4h |
| Pure Rust embeddings | 🔴 Alta | 3-5 giorni |
| Temporal decay | 🟢 Bassa | 4h |
| Analytics | 🟡 Media | 1-2 giorni |
| **TOTALE FASE 3** | - | **5-10 giorni** |

### 6.2 Risorse Necessarie

**Sviluppatore**:
- Esperienza con Rust + async/tokio
- Familiarità con rig-rs
- Conoscenza base di vector databases

**Infrastruttura**:
- Storage locale: ~100MB per 10k interactions (JSONL + LanceDB)
- RAM: +200MB per embeddings model in memoria
- CPU: embedding generation ~50ms per record

**API Costs** (Fase 2+):
- Intent generation: ~$0.01 per 100 comandi
- Con caching: ~$5-10/mese per uso normale
- Fallback locale disponibile

---

## 7. Analisi Costi/Benefici

### 7.1 Benefici

#### 🚀 Benefici Immediati (Fase 1)

1. **Persistenza Cross-Session**
   ```
   Scenario attuale:
   - Restart backend → memoria persa

   Con memory system:
   - Restart backend → ricorda tutto
   - Accumula conoscenza nel tempo
   ```

2. **Risolve "History Confusion"**
   ```
   Problema attuale:
   // "history was causing LLM confusion"

   Soluzione:
   - Pre-retrieval semantico: solo top-3 rilevanti
   - No più storia lineare infinita
   ```

3. **Context-Aware per Progetto**
   ```
   /home/user/django-app → ricorda comandi Django
   /home/user/rust-project → ricorda comandi Cargo
   ```

#### 🎯 Benefici a Lungo Termine (Fase 2)

4. **Apprendimento Continuo**
   - Ogni comando eseguito → migliora suggestions
   - Pattern detection automatico
   - Personalizzazione sul workflow utente

5. **Riduzione Cognitive Load**
   - Non serve ricordare comandi complessi
   - "come ho fatto quella cosa settimana scorsa?"
   - Agent suggerisce basandosi su storia

6. **Efficienza Token**
   - Pre-retrieval: solo 3 esempi rilevanti
   - Più efficiente che mandare tutta la history
   - Costi API più bassi

#### 💎 Killer Features (Fase 3)

7. **Agent Proattivo**
   - "Vedo che stai facendo X, settimana scorsa hai usato Y"
   - Pattern recognition e best practices
   - Error prevention based on history

### 7.2 Costi e Rischi

#### ⚠️ Costi di Sviluppo

1. **Tempo di sviluppo**: 7-10 giorni (MVP + Semantic)
2. **Complessità codebase**: +20% (nuovo modulo memory/)
3. **Manutenzione**: +10% (più superficie per bug)

#### ⚠️ Costi Runtime

1. **Latency**:
   - Simple search (Fase 1): +10-20ms
   - Semantic search (Fase 2): +100-200ms
   - Intent generation: +2-3s (solo se cache miss)

2. **Storage**:
   - JSONL: ~10KB per interaction
   - LanceDB: ~5KB per embedding
   - 10k interactions = ~150MB

3. **API Costs**:
   - Intent generation: ~$0.01 per 100 comandi
   - Con caching: ~$5-10/mese

#### ⚠️ Rischi Tecnici

1. **Python dependency** (Fase 2):
   - Embeddings richiedono Python subprocess
   - Mitigation: fallback su simple search
   - Alternativa: pure Rust (Fase 3)

2. **LanceDB stability**:
   - Relativamente nuovo
   - Mitigation: JSONL è source of truth

3. **Storage growth**:
   - Potenzialmente unbounded
   - Mitigation: rotation policy (es. 6 mesi)

### 7.3 ROI Analysis

**Investimento**: 10 giorni sviluppo

**Valore per utente**:
- Time saved: ~5-10 minuti/giorno (no need to search history/docs)
- Cognitive load reduction: significativo
- Learning curve reduction: agent diventa più smart nel tempo

**Valore per prodotto**:
- Differenziazione competitiva: memoria contestuale è rara
- User engagement: migliore UX = più uso
- Retention: gli utenti non vogliono perdere la "memoria" dell'agent

**Break-even**: Dopo ~2-3 settimane di uso normale

---

## 8. Metriche di Successo

### 8.1 KPI Tecnici

**Fase 1 (MVP)**:
- ✅ Persistenza: 100% interazioni salvate su disco
- ✅ Retrieval accuracy: >70% match su keyword
- ✅ Latency: <50ms per search
- ✅ Crash recovery: 100% dopo restart

**Fase 2 (Semantic)**:
- ✅ Semantic accuracy: >85% match su intent
- ✅ Latency: <200ms per pre-retrieval
- ✅ Working dir filtering: 100% accuracy
- ✅ API cost: <$10/mese per utente normale

### 8.2 KPI User Experience

**Misurazioni**:
1. **Suggestion Hit Rate**
   - % di volte che l'agent suggerisce il comando corretto
   - Target: >80% dopo 1 settimana di uso

2. **Time to Command**
   - Tempo medio per ottenere il comando desiderato
   - Target: -50% rispetto a baseline

3. **User Satisfaction**
   - Feedback su suggestions (thumbs up/down)
   - Target: >90% positive

4. **Command Reuse Rate**
   - % di comandi che sono varianti di comandi passati
   - Baseline: misurare prima dell'implementazione
   - Expected: >40%

### 8.3 Monitoring

**Logs da tracciare**:
```rust
// Pre-retrieval
tracing::info!(
    query = %prompt,
    matches = results.len(),
    top_similarity = results.first().map(|r| r.similarity),
    latency_ms = elapsed.as_millis(),
    "Memory pre-retrieval completed"
);

// Storage
tracing::info!(
    command = %cmd,
    intent = %intent,
    working_dir = %wd,
    "Interaction stored in memory"
);

// Search tool usage
tracing::info!(
    tool = "memory_search",
    query = %args.query,
    results = matches.len(),
    "Agent used memory search tool"
);
```

---

## 9. Decisioni Architetturali

### 9.1 Embeddings: Python vs Rust

**Raccomandazione**: **Python per MVP** (Fase 1-2), Rust per ottimizzazione (Fase 3)

| Aspetto | Python (sentence-transformers) | Rust (candle + ONNX) |
|---------|--------------------------------|----------------------|
| **Complessità** | 🟢 Bassa | 🔴 Alta |
| **Time to implement** | 1-2 giorni | 3-5 giorni |
| **Performance** | 🟡 ~100ms | 🟢 ~50ms |
| **Dependencies** | Python runtime | Pure Rust |
| **Maturità** | 🟢 Stabile | 🟡 Emergente |
| **Maintenance** | 🟡 Media | 🟢 Bassa |

**Strategia**: Start Python, migrate to Rust later if needed.

### 9.2 Intent Generation: API vs Local

**Raccomandazione**: **Hybrid approach**

```rust
async fn generate_intent(command: &str) -> Result<String> {
    // Check cache first
    if let Some(cached) = intent_cache.get(command) {
        return Ok(cached.clone());
    }

    // Try Claude API
    match claude_api_intent(command).await {
        Ok(intent) => {
            intent_cache.insert(command.to_string(), intent.clone());
            Ok(intent)
        }
        Err(e) => {
            tracing::warn!("Claude API failed, using fallback: {:?}", e);
            Ok(fallback_intent(command))  // Regex-based
        }
    }
}
```

**Vantaggi**:
- Best of both worlds
- Resiliente a API failures
- Costi contenuti con caching

### 9.3 Storage: JSONL + LanceDB vs Single Store

**Raccomandazione**: **Dual storage** (come memory_test POC)

**Rationale**:
- JSONL: source of truth, human-readable, backup facile
- LanceDB: ottimizzato per vector search, ma opaco
- Separazione concerns: data vs index

**Alternative considerate**:
- ❌ Solo LanceDB: difficile debug, difficile backup
- ❌ Solo JSONL: search lento a scala
- ❌ SQLite + vectors: più complesso, nessun beneficio

### 9.4 Working Directory: Auto-detect vs Explicit

**Raccomandazione**: **Auto-detect da environment**

```rust
fn extract_working_dir(input: &RunInput) -> Option<String> {
    // 1. Check se specificato esplicitamente nel input metadata
    if let Some(wd) = input.metadata.get("working_dir") {
        return wd.as_str().map(|s| s.to_string());
    }

    // 2. Auto-detect da environment
    std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
}
```

**Benefici**:
- Zero friction per utente
- Automatic context per progetto
- Fallback sempre disponibile

---

## 10. Testing Strategy

### 10.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_store_add_and_retrieve() {
        let store = MemoryStore::new_temp().await.unwrap();

        let record = InteractionRecord {
            input: "docker ps".to_string(),
            intent: "list running containers".to_string(),
            data_type: "command".to_string(),
            // ...
        };

        store.add_interaction(record.clone()).await.unwrap();

        let results = store.search_similar("show containers", 1, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].record.input, "docker ps");
    }

    #[tokio::test]
    async fn test_working_dir_filtering() {
        let store = MemoryStore::new_temp().await.unwrap();

        // Add command in project A
        store.add_interaction(InteractionRecord {
            input: "npm start".to_string(),
            context: hashmap! { "working_dir" => "/home/user/project-a" },
            // ...
        }).await.unwrap();

        // Add command in project B
        store.add_interaction(InteractionRecord {
            input: "cargo run".to_string(),
            context: hashmap! { "working_dir" => "/home/user/project-b" },
            // ...
        }).await.unwrap();

        // Search in project A
        let results = store.search_similar(
            "start server",
            5,
            Some("/home/user/project-a")
        ).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].record.input, "npm start");
    }
}
```

### 10.2 Integration Tests

```rust
#[tokio::test]
async fn test_rig_engine_with_memory_e2e() {
    let config = RigEngineConfig::new("test-api-key");
    let engine = RigEngine::new(config)
        .with_memory()  // Enable memory
        .await
        .unwrap();

    let thread_id = engine.create_thread(None).await.unwrap();

    // 1. Execute first command
    let input1 = RunInput::single_user_message("list docker containers");
    let stream1 = engine.stream_run(&thread_id, input1).await.unwrap();
    // ... collect events, verify "docker ps" suggested

    // 2. Simulate restart (clear in-memory state but keep disk storage)
    drop(engine);
    let engine = RigEngine::new(config).with_memory().await.unwrap();

    // 3. Execute similar command in new session
    let thread_id2 = engine.create_thread(None).await.unwrap();
    let input2 = RunInput::single_user_message("show running containers");
    let stream2 = engine.stream_run(&thread_id2, input2).await.unwrap();

    // Verify pre-retrieval found previous "docker ps" command
    // ... assertions
}
```

### 10.3 Performance Tests

```rust
#[tokio::test]
async fn bench_memory_search_latency() {
    let store = create_store_with_1000_records().await;

    let start = std::time::Instant::now();
    let results = store.search_similar("list files", 5, None).await.unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed.as_millis() < 200, "Search too slow: {:?}", elapsed);
    assert!(results.len() > 0, "No results found");
}
```

---

## 11. Deployment e Rollout

### 11.1 Feature Flag

```rust
// RigEngineConfig
pub struct RigEngineConfig {
    // Existing fields...

    /// Enable memory system (feature flag)
    #[serde(default)]
    pub memory_enabled: bool,

    /// Path to memory data directory
    #[serde(default = "default_memory_dir")]
    pub memory_data_dir: PathBuf,
}

impl RigEngineConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            // ...
            memory_enabled: std::env::var("MEMORY_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            memory_data_dir: std::env::var("MEMORY_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("./data/memory")),
        })
    }
}
```

### 11.2 Rollout Plan

**Week 1-2: Development (Fase 1 MVP)**
- Develop + test in feature branch
- Internal testing con small dataset

**Week 3: Alpha Testing**
- Enable for 10% users: `MEMORY_ENABLED=true` for subset
- Monitor metrics: latency, accuracy, errors
- Collect feedback

**Week 4: Beta Testing**
- Enable for 50% users if alpha successful
- A/B test: memory ON vs OFF
- Compare metrics

**Week 5: General Availability**
- Enable for 100% users
- Monitor for 1 week
- Fallback plan ready se problemi

### 11.3 Monitoring e Alerting

```rust
// Prometheus metrics
lazy_static! {
    static ref MEMORY_SEARCH_DURATION: Histogram = register_histogram!(
        "memory_search_duration_seconds",
        "Duration of memory search operations"
    ).unwrap();

    static ref MEMORY_SEARCH_RESULTS: Histogram = register_histogram!(
        "memory_search_results_count",
        "Number of results returned by memory search"
    ).unwrap();

    static ref MEMORY_ERRORS: Counter = register_counter!(
        "memory_errors_total",
        "Total number of memory system errors"
    ).unwrap();
}
```

**Alerts**:
- ⚠️ Search latency >500ms
- ⚠️ Error rate >1%
- ⚠️ Storage size >10GB

---

## 12. Conclusioni e Next Steps

### 12.1 Raccomandazione Finale

✅ **PROCEDI con approccio incrementale MVP-first**

**Rationale**:
1. **Use case perfetto**: terminal DevOps beneficia enormemente da memoria contestuale
2. **ROI alto**: 10 giorni sviluppo vs benefici long-term significativi
3. **Rischio controllato**: MVP può essere fatto in 2-3 giorni, validato, poi espanso
4. **Architettura compatibile**: RigEngine già modulare, integrazione pulita
5. **Differenziazione competitiva**: feature rara e ad alto valore

### 12.2 Immediate Next Steps

**Week 1**:
1. ✅ Create feature branch: `feature/memory-system-mvp`
2. ✅ Implement JSONL storage module
3. ✅ Add simple text-based search
4. ✅ Integrate pre-retrieval hook in orchestrator
5. ✅ Write unit tests

**Week 2**:
6. ✅ Test MVP internamente
7. ✅ Collect metrics su command reuse rate
8. ✅ Decide: continue to Fase 2 or iterate on MVP
9. ✅ Document learnings in questo file

**Decision Point**: Se MVP mostra >40% command reuse → proceed to Fase 2

### 12.3 Success Criteria

**MVP è successo se**:
- ✅ Persistenza funziona (no data loss su restart)
- ✅ Pre-retrieval trova match in >70% dei casi
- ✅ Latency <50ms
- ✅ Nessun crash o memory leak

**Fase 2 è successo se**:
- ✅ Semantic search accuracy >85%
- ✅ Working dir filtering 100% accurate
- ✅ API costs <$10/mese per utente

### 12.4 Long-term Vision

**6 mesi**:
- Memory system in produzione
- 10k+ interactions per utente
- Agent sempre più smart e personalizzato

**12 mesi**:
- Pattern detection automatico
- Proactive suggestions
- Best practices learning
- Team collaboration (shared memory)

---

## 13. Riferimenti

### 13.1 Codebase References

**Memory Test POC**:
- Path: `/home/crist/memory_test`
- Key files:
  - `src/memory/store.py` - Dual storage implementation
  - `src/memory/intent_generator.py` - Intent generation
  - `src/agent/agent.py` - Pre-retrieval integration
  - `CLAUDE.md` - Comprehensive documentation

**RigEngine**:
- Path: `/home/crist/infraware-terminal/crates/backend-engine`
- Key files:
  - `src/adapters/rig/engine.rs` - Engine implementation
  - `src/adapters/rig/orchestrator.rs` - Run stream creation
  - `src/adapters/rig/state.rs` - StateStore
  - `src/adapters/rig/tools/` - Existing tools

### 13.2 External Resources

**LanceDB**:
- Docs: https://lancedb.github.io/lancedb/
- Rust crate: https://crates.io/crates/lancedb

**Sentence Transformers**:
- Docs: https://www.sbert.net/
- Models: https://huggingface.co/sentence-transformers

**Rig-rs**:
- Repo: https://github.com/0xPlaygrounds/rig
- Docs: https://docs.rig.rs/

### 13.3 Related Documents

- `BACKEND_ARCHITECTURE.md` - Overall architecture
- `RIGENGINE_PLAN.md` - RigEngine implementation plan
- `NATIVE_TOOLS_INTEGRATION_PLAN.md` - Tool integration patterns

---

## Appendix A: Code Snippets

### A.1 Complete MemoryStore Interface

```rust
/// Memory store for semantic search of past interactions
pub struct MemoryStore {
    /// JSONL storage backend
    jsonl_path: PathBuf,

    /// LanceDB vector store
    lancedb: Arc<RwLock<LanceDbClient>>,

    /// Embedding engine
    embeddings: Arc<dyn EmbeddingEngine>,

    /// Intent generator
    intent_gen: Arc<IntentGenerator>,

    /// In-memory cache
    cache: Arc<RwLock<LruCache<String, String>>>,
}

impl MemoryStore {
    /// Create new memory store
    pub async fn new(data_dir: PathBuf) -> Result<Self>;

    /// Add interaction to memory
    pub async fn add_interaction(&self, record: InteractionRecord) -> Result<String>;

    /// Search for similar interactions
    pub async fn search_similar(
        &self,
        query: &str,
        top_k: usize,
        working_dir: Option<&str>
    ) -> Result<Vec<SearchResult>>;

    /// Generate intent for input
    pub async fn generate_intent(
        &self,
        input: &str,
        data_type: &str
    ) -> Result<String>;

    /// Get statistics
    pub async fn stats(&self) -> MemoryStats;
}
```

### A.2 Context Formatting

```rust
fn format_memory_context_for_agent(results: &[SearchResult]) -> String {
    if results.is_empty() {
        return String::new();
    }

    let mut context = String::from("Based on your memory, you previously:\n\n");

    for (idx, result) in results.iter().enumerate() {
        let record = &result.record;
        let similarity_pct = (result.similarity * 100.0) as u32;

        context.push_str(&format!(
            "{}. [{}% match] Ran: `{}` → {}\n   Working dir: {}\n   Timestamp: {}\n\n",
            idx + 1,
            similarity_pct,
            record.input,
            record.intent,
            record.context.get("working_dir")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown"),
            record.timestamp
        ));
    }

    context.push_str("Consider this context when responding to the current query.\n");
    context
}
```

---

**Document Version**: 1.0
**Last Updated**: 2026-02-06
**Status**: Proposta per review
**Next Review**: After MVP completion
