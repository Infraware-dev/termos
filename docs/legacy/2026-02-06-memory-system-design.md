# Memory System Design - RigEngine

**Data**: 2026-02-06
**Status**: Validato via brainstorming
**Riferimento**: `docs/MEMORY_SYSTEM_INTEGRATION_PLAN.md` (analisi iniziale)

---

## Decisioni di Design

| Decisione | Scelta | Rationale |
|-----------|--------|-----------|
| Approccio | Fase 1 MVP → Fase 2 Semantic | Incrementale, validazione anticipata |
| Embeddings | Pure Rust (candle + ONNX) | Coerente con stack Rust, zero dipendenze Python |
| Scope cattura | Comandi eseguiti + query NL | Due flussi con `DataType` enum |
| Storage path | `~/.local/share/infraware/memory/` | XDG-compliant, centralizzato |
| Architettura | `MemoryStore` separato da `StateStore` | SOLID: separazione concerns |
| Pattern | Strategy con generics + type alias | Zero-cost, swap a compile-time via feature |
| Feature flag | `rig-memory` (sub-feature di `rig`) | Accoppiato a RigEngine, unico engine nativo |
| Orchestrator | `OrchestratorContext` struct condivisa | Risolve `cfg` su parametri, firme pulite |
| Rust style | 2024 edition idioms | `.as_ref()` anziché `ref` keyword, let chains, ecc. |

---

## Struttura Moduli

```
crates/backend-engine/src/adapters/rig/
├── memory/                    # Nuovo modulo (feature: rig-memory)
│   ├── mod.rs                 # Public API + MemoryStore<S, E, I> + type alias ActiveMemory
│   ├── traits.rs              # MemoryStorage, EmbeddingEngine, IntentGenerator
│   ├── models.rs              # InteractionRecord, SearchResult, DataType
│   ├── storage/
│   │   ├── mod.rs
│   │   └── jsonl.rs           # Fase 1: JsonlStorage impl MemoryStorage
│   ├── embeddings/
│   │   ├── mod.rs
│   │   ├── noop.rs            # Fase 1: NoopEmbedding (placeholder)
│   │   └── candle.rs          # Fase 2: CandleEmbedding (pure Rust)
│   └── intent/
│       ├── mod.rs
│       ├── regex.rs           # Fase 1: RegexIntentGenerator
│       └── claude.rs          # Fase 2: ClaudeIntentGenerator
├── engine.rs                  # RigEngine + memory: Option<Arc<ActiveMemory>>
├── orchestrator.rs            # OrchestratorContext + pre-retrieval + cattura
└── tools/
    └── memory.rs              # Fase 3: MemorySearchTool
```

---

## Traits Core

```rust
// traits.rs

#[trait_variant::make(Send)]
pub trait MemoryStorage: Send + Sync {
    async fn append(&self, record: &InteractionRecord) -> Result<()>;
    async fn get_by_id(&self, id: &str) -> Result<Option<InteractionRecord>>;
    async fn search(&self, query: &str, top_k: usize, working_dir: Option<&str>) -> Result<Vec<SearchResult>>;
    async fn list_recent(&self, limit: usize) -> Result<Vec<InteractionRecord>>;
    async fn count(&self) -> Result<usize>;
}

pub trait EmbeddingEngine: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    fn dimension(&self) -> usize;
}

#[trait_variant::make(Send)]
pub trait IntentGenerator: Send + Sync {
    async fn generate(&self, input: &str, data_type: DataType) -> Result<String>;
}
```

---

## Modelli Dati

```rust
// models.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionRecord {
    pub id: String,                    // UUID v4
    pub timestamp: String,             // ISO-8601
    pub data_type: DataType,
    pub intent: String,                // Descrizione semantica
    pub input: String,                 // Comando o query originale
    pub stderr: bool,                  // Se l'esecuzione ha avuto errori
    pub context: InteractionContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataType {
    #[serde(rename = "command")]
    Command,
    #[serde(rename = "natural_language")]
    NaturalLanguage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub record: InteractionRecord,
    pub similarity: f32,               // 0.0 - 1.0
}
```

---

## MemoryStore (Composizione Strategy)

```rust
// mod.rs

pub struct MemoryStore<S, E, I>
where
    S: MemoryStorage,
    E: EmbeddingEngine,
    I: IntentGenerator,
{
    storage: S,
    embeddings: E,
    intent_gen: I,
    data_dir: PathBuf,
}

// Type alias per fase - swap a compile-time
// Fase 1
pub type ActiveMemory = MemoryStore<JsonlStorage, NoopEmbedding, RegexIntentGenerator>;

// Fase 2 (futuro): cambi solo questo
// pub type ActiveMemory = MemoryStore<LanceDbStorage, CandleEmbedding, ClaudeIntentGenerator>;
```

---

## Implementazioni Fase 1

### JSONL Storage

```rust
// storage/jsonl.rs

pub struct JsonlStorage {
    path: PathBuf,
}

impl JsonlStorage {
    pub fn new(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        Ok(Self {
            path: data_dir.join("interactions.jsonl"),
        })
    }
}

impl MemoryStorage for JsonlStorage {
    async fn append(&self, record: &InteractionRecord) -> Result<()> {
        // tokio::fs append + serde_json::to_string + newline
    }

    async fn search(&self, query: &str, top_k: usize, working_dir: Option<&str>) -> Result<Vec<SearchResult>> {
        // Fase 1: carica in memoria, text matching
        // Fase 2: delega a LanceDB per vector search
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<InteractionRecord>> {
        // Scan JSONL, deserialize, match id
    }

    async fn list_recent(&self, limit: usize) -> Result<Vec<InteractionRecord>> {
        // Read last N lines from JSONL
    }

    async fn count(&self) -> Result<usize> {
        // Count lines in JSONL
    }
}
```

### Noop Embedding

```rust
// embeddings/noop.rs

pub struct NoopEmbedding;

impl EmbeddingEngine for NoopEmbedding {
    fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![])  // Nessun embedding in Fase 1
    }

    fn dimension(&self) -> usize { 0 }
}
```

### Regex Intent Generator

```rust
// intent/regex.rs

pub struct RegexIntentGenerator {
    question_words: HashSet<&'static str>,
}

impl IntentGenerator for RegexIntentGenerator {
    async fn generate(&self, input: &str, data_type: DataType) -> Result<String> {
        match data_type {
            DataType::Command => {
                // Estrai verbo base del comando (skip sudo, nohup, ecc.)
                // "docker-compose up -d" → "executed docker-compose up"
                // "pip install flask" → "executed pip install"
                Ok(fallback_command_intent(input))
            }
            DataType::NaturalLanguage => {
                // Rimuovi question words, normalizza
                // "how can I install redis" → "install redis"
                Ok(normalize_natural_language(input, &self.question_words))
            }
        }
    }
}
```

---

## Configurazione

```rust
// config.rs (estensione)

#[cfg(feature = "rig-memory")]
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    pub enabled: bool,
    pub data_dir: PathBuf,          // Default: ~/.local/share/infraware/memory/
    pub max_results: usize,          // Default: 3 (per pre-retrieval)
}

#[cfg(feature = "rig-memory")]
impl MemoryConfig {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("MEMORY_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true),  // Abilitato di default se feature attiva
            data_dir: std::env::var("MEMORY_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| default_data_dir()),
            max_results: std::env::var("MEMORY_MAX_RESULTS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3),
        }
    }
}

fn default_data_dir() -> PathBuf {
    dirs::data_dir()                    // ~/.local/share/ su Linux
        .unwrap_or_else(|| PathBuf::from("."))
        .join("infraware")
        .join("memory")
}
```

### Cargo.toml

```toml
[dependencies]
dirs = { version = "6", optional = true }
chrono = { version = "0.4", optional = true }

[features]
rig = ["rig-core", "async-stream", "regex", "schemars"]
rig-memory = ["rig", "dirs", "chrono"]
```

### Environment Variables

```bash
# Memory system (richiede feature rig-memory)
MEMORY_ENABLED=true              # Default: true (se feature attiva)
MEMORY_DATA_DIR=~/.local/share/infraware/memory/  # Default: XDG data dir
MEMORY_MAX_RESULTS=3             # Default: 3
```

---

## Integrazione RigEngine

### Bootstrap

```rust
// engine.rs

pub struct RigEngine {
    config: Arc<RigEngineConfig>,
    client: Arc<anthropic::Client>,
    state: Arc<StateStore>,
    memory: Option<Arc<ActiveMemory>>,
}

impl RigEngine {
    pub fn new(config: RigEngineConfig) -> Result<Self, EngineError> {
        // ... existing client creation ...

        #[cfg(feature = "rig-memory")]
        let memory = {
            let mem_config = MemoryConfig::from_env();
            if mem_config.enabled {
                match MemoryStore::new(&mem_config) {
                    Ok(store) => {
                        tracing::info!(data_dir = %mem_config.data_dir.display(), "Memory system initialized");
                        Some(Arc::new(store))
                    }
                    Err(e) => {
                        tracing::warn!(error = ?e, "Memory system failed to initialize, continuing without");
                        None
                    }
                }
            } else {
                None
            }
        };

        #[cfg(not(feature = "rig-memory"))]
        let memory = None::<Arc<ActiveMemory>>;

        Ok(Self {
            config: Arc::new(config),
            client: Arc::new(client),
            state: Arc::new(StateStore::new()),
            memory,
        })
    }
}
```

### OrchestratorContext

```rust
// orchestrator.rs

/// Shared context for orchestrator functions
pub struct OrchestratorContext {
    pub config: Arc<RigEngineConfig>,
    pub client: Arc<anthropic::Client>,
    pub state: Arc<StateStore>,
    #[cfg(feature = "rig-memory")]
    pub memory: Option<Arc<ActiveMemory>>,
}

// Firme pulite
pub fn create_run_stream(
    ctx: Arc<OrchestratorContext>,
    thread_id: ThreadId,
    input: RunInput,
    run_id: String,
) -> EventStream;

pub fn create_resume_stream(
    ctx: Arc<OrchestratorContext>,
    thread_id: ThreadId,
    response: ResumeResponse,
    run_id: String,
) -> EventStream;
```

---

## Punti di Aggancio (Pre-Retrieval e Cattura)

### Pre-Retrieval (create_run_stream)

```rust
// Dentro create_run_stream(), prima della chiamata agent

// 1. Estrai prompt (codice esistente)
let prompt = input.messages.iter()
    .filter(|m| m.role == MessageRole::User)
    .map(|m| m.content.as_str())
    .collect::<Vec<_>>()
    .join("\n");

// 2. Pre-retrieval
let memory_context = if let Some(memory) = ctx.memory.as_ref() {
    match memory.search(&prompt, 3, working_dir.as_deref()).await {
        Ok(results) if !results.is_empty() => format_for_preamble(&results),
        _ => String::new(),
    }
} else {
    String::new()
};

// 3. Agent con preamble arricchito
let preamble = if memory_context.is_empty() {
    config.system_prompt.clone()
} else {
    format!("{}\n\n## RELEVANT PAST INTERACTIONS\n{}", config.system_prompt, memory_context)
};
```

### Cattura Comandi (create_resume_stream)

```rust
// Dopo CommandExecutionResult::Completed(output)

if let Some(memory) = ctx.memory.as_ref() {
    let intent = memory.generate_intent(&command, DataType::Command).await
        .unwrap_or_else(|_| format!("executed {}", command));

    let record = InteractionRecord::new(
        DataType::Command,
        intent,
        command.clone(),
        output.contains("Exit code:"),  // stderr detection
        working_dir.clone(),
    );

    if let Err(e) = memory.add(&record).await {
        tracing::warn!(error = ?e, "Failed to store command in memory");
    }
}
```

### Cattura Query NL (create_run_stream)

```rust
// Dopo aver estratto il prompt utente

if let Some(memory) = ctx.memory.as_ref() {
    let intent = memory.generate_intent(&prompt, DataType::NaturalLanguage).await
        .unwrap_or_else(|_| prompt.clone());

    let record = InteractionRecord::new(
        DataType::NaturalLanguage,
        intent,
        prompt.clone(),
        false,
        working_dir.clone(),
    );

    let _ = memory.add(&record).await;  // Best-effort, non blocca il flusso
}
```

### Riepilogo Punti di Aggancio

| Punto | Dove | Cosa | Errore |
|-------|------|------|--------|
| Pre-retrieval | `create_run_stream()` prima di agent | Cerca top-3 simili → arricchisci preamble | Fallback: nessun context |
| Cattura query | `create_run_stream()` al ricevimento prompt | Salva query NL dell'utente | Best-effort, log warn |
| Cattura comandi | `create_resume_stream()` dopo esecuzione | Salva comando + esito | Best-effort, log warn |

---

## Fasi di Implementazione

### Fase 1: MVP (2-3 giorni)

- `traits.rs` - 3 trait definitions
- `models.rs` - InteractionRecord, SearchResult, DataType
- `storage/jsonl.rs` - JsonlStorage (text matching)
- `embeddings/noop.rs` - NoopEmbedding placeholder
- `intent/regex.rs` - RegexIntentGenerator
- `mod.rs` - MemoryStore + ActiveMemory type alias
- `config.rs` - MemoryConfig + env vars
- `engine.rs` - Aggiunta campo memory + bootstrap
- `orchestrator.rs` - OrchestratorContext + 3 punti di aggancio

### Fase 2: Semantic Search (5-7 giorni)

- `storage/lancedb.rs` - LanceDbStorage impl MemoryStorage
- `embeddings/candle.rs` - CandleEmbedding (all-MiniLM-L6-v2 ONNX)
- `intent/claude.rs` - ClaudeIntentGenerator (usa existing Anthropic client)
- Aggiornamento type alias ActiveMemory
- Working directory filtering in LanceDB

### Fase 3: Advanced (opzionale)

- `tools/memory.rs` - MemorySearchTool per agent
- Temporal decay
- Analytics e metriche
- Rotation/cleanup policy

---

**Document Version**: 1.0
**Last Updated**: 2026-02-06
**Status**: Design validato, pronto per implementazione Fase 1
