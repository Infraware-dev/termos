# Design Patterns - Infraware Terminal

**Autore**: Design Pattern Refactoring Plan
**Data**: Week 2 - M1
**Versione**: 1.0
**Status**: In Progress

---

## Indice

1. [Introduzione](#introduzione)
2. [Pattern Implementati](#pattern-implementati)
3. [Pattern da Implementare](#pattern-da-implementare)
4. [Roadmap di Implementazione](#roadmap-di-implementazione)
5. [Guide Pratiche](#guide-pratiche)
6. [Appendici](#appendici)

---

## Introduzione

### Overview

Infraware Terminal sta evolvendo da un MVP (M1) verso un'architettura più robusta e flessibile per supportare le funzionalità avanzate di M2 e M3. Questo documento descrive gli 8 design pattern strategici che stiamo implementando per:

- **Migliorare la flessibilità** - Facile aggiungere nuove funzionalità
- **Aumentare la testabilità** - Dependency injection e mocking
- **Ridurre l'accoppiamento** - Separazione delle responsabilità
- **Preparare per M2/M3** - Plugin system, configurazione, telemetria

### Obiettivi

| Obiettivo | Target | Status |
|-----------|--------|--------|
| LLM reale configurabile | ✅ Via env var | **COMPLETATO** |
| Supporto config file | 📋 M2 | Planned |
| +50% test coverage | 🎯 M2 | In Progress |
| -30% coupling tra moduli | 🎯 M2 | In Progress |
| Plugin system ready | 📋 M3 | Planned |

### Principi SOLID Applicati

- **S** - Single Responsibility: Ogni classe ha una sola responsabilità
- **O** - Open/Closed: Aperto all'estensione, chiuso alla modifica
- **L** - Liskov Substitution: Le implementazioni sono intercambiabili
- **I** - Interface Segregation: Trait piccoli e focalizzati
- **D** - Dependency Inversion: Dipendenze su astrazioni, non implementazioni

---

## Pattern Implementati

### ✅ 1. Trait Object Pattern - LLM Client

**Status**: ✅ **COMPLETATO** (Fase 1 - Week 2)
**Priorità**: CRITICAL
**Effort**: Low
**Impact**: Production-Ready

#### Problema

Il client LLM era hardcoded come `MockLLMClient`, rendendo impossibile:
- Usare un backend LLM reale in produzione
- Testare con diversi provider LLM
- Configurare il client via environment variable

```rust
// ❌ PRIMA: Dipendenza hardcoded
struct InfrawareTerminal {
    llm_client: MockLLMClient,  // Non modificabile!
}

impl InfrawareTerminal {
    fn new() -> Result<Self> {
        Ok(Self {
            llm_client: MockLLMClient,
        })
    }
}
```

#### Soluzione

Implementato il **Trait Object Pattern** con Dependency Injection:

```rust
// ✅ DOPO: Astrazione tramite trait

// 1. Definizione del trait
#[async_trait]
pub trait LLMClientTrait: Send + Sync {
    async fn query(&self, text: &str) -> Result<String>;
    async fn query_with_context(&self, text: &str, context: Option<String>) -> Result<String>;
    async fn query_with_history(&self, text: &str, history: &[String]) -> Result<String>;
}

// 2. Implementazioni concrete
pub struct MockLLMClient;

#[async_trait]
impl LLMClientTrait for MockLLMClient {
    async fn query(&self, text: &str) -> Result<String> {
        // Mock responses
        Ok("Mock response".to_string())
    }
}

pub struct HttpLLMClient {
    base_url: String,
    client: reqwest::Client,
}

#[async_trait]
impl LLMClientTrait for HttpLLMClient {
    async fn query(&self, text: &str) -> Result<String> {
        // Real HTTP request
        let response = self.client
            .post(&format!("{}/query", self.base_url))
            .json(&LLMRequest { query: text.to_string(), context: None })
            .send()
            .await?;

        Ok(response.json::<LLMResponse>().await?.text)
    }
}

// 3. Dependency Injection
struct InfrawareTerminal {
    llm_client: Arc<dyn LLMClientTrait>,  // Trait object!
}

impl InfrawareTerminal {
    fn new_with_client(llm_client: Arc<dyn LLMClientTrait>) -> Result<Self> {
        Ok(Self { llm_client })
    }
}

// 4. Configurazione in main()
#[tokio::main]
async fn main() -> Result<()> {
    let llm_client: Arc<dyn LLMClientTrait> = match std::env::var("INFRAWARE_LLM_URL") {
        Ok(url) => Arc::new(HttpLLMClient::new(url)),
        Err(_) => Arc::new(MockLLMClient::new()),
    };

    let mut terminal = InfrawareTerminal::new_with_client(llm_client)?;
    terminal.run().await
}
```

#### Benefici

| Beneficio | Descrizione |
|-----------|-------------|
| **Produzione Ready** | Possiamo usare LLM reale via `INFRAWARE_LLM_URL` |
| **Testabilità** | Facile usare mock nei test |
| **Estensibilità** | Aggiungere OpenAI/Claude è solo implementare il trait |
| **Configurabilità** | Switch tra backend senza ricompilare |

#### Uso Pratico

```bash
# Sviluppo (mock)
cargo run

# Produzione (HTTP backend)
INFRAWARE_LLM_URL=http://llm.example.com:8080 cargo run

# Testing
INFRAWARE_LLM_URL=http://localhost:3000 cargo test --integration
```

#### File Modificati

- `src/llm/client.rs` - Trait e implementazioni
- `src/llm/mod.rs` - Exports
- `src/main.rs` - Dependency injection
- `Cargo.toml` - Aggiunto `async-trait`
- `tests/integration_tests.rs` - Aggiunto import trait

---

## Pattern da Implementare

### 📋 2. Builder Pattern - InfrawareTerminal

**Status**: 📋 **PLANNED** (Fase 1 - Week 2)
**Priorità**: HIGH
**Effort**: Low-Medium
**Impact**: Testabilità + Configurazione

#### Problema

`InfrawareTerminal::new()` hardcoda tutte le dipendenze, rendendo difficile:
- Testare con configurazioni diverse
- Caricare da file di configurazione (M2)
- Iniettare mock per testing

```rust
// ❌ PROBLEMA ATTUALE
impl InfrawareTerminal {
    fn new_with_client(llm_client: Arc<dyn LLMClientTrait>) -> Result<Self> {
        Ok(Self {
            ui: TerminalUI::new()?,           // Hardcoded
            state: TerminalState::new(),      // Hardcoded
            classifier: InputClassifier::new(), // Hardcoded
            event_handler: EventHandler::new(), // Hardcoded
            llm_client,
            renderer: ResponseRenderer::new(), // Hardcoded
        })
    }
}
```

#### Soluzione Proposta

```rust
pub struct InfrawareTerminalBuilder {
    ui: Option<TerminalUI>,
    state: Option<TerminalState>,
    classifier: Option<InputClassifier>,
    event_handler: Option<EventHandler>,
    llm_client: Option<Arc<dyn LLMClientTrait>>,
    renderer: Option<ResponseRenderer>,
}

impl InfrawareTerminalBuilder {
    pub fn new() -> Self {
        Self {
            ui: None,
            state: None,
            classifier: None,
            event_handler: None,
            llm_client: None,
            renderer: None,
        }
    }

    // Fluent API
    pub fn with_ui(mut self, ui: TerminalUI) -> Self {
        self.ui = Some(ui);
        self
    }

    pub fn with_llm_client(mut self, client: Arc<dyn LLMClientTrait>) -> Self {
        self.llm_client = Some(client);
        self
    }

    pub fn with_classifier(mut self, classifier: InputClassifier) -> Self {
        self.classifier = Some(classifier);
        self
    }

    // ... altri builder methods

    pub fn build(self) -> Result<InfrawareTerminal> {
        Ok(InfrawareTerminal {
            ui: self.ui.unwrap_or_else(|| TerminalUI::new().unwrap()),
            state: self.state.unwrap_or_else(TerminalState::new),
            classifier: self.classifier.unwrap_or_else(InputClassifier::new),
            event_handler: self.event_handler.unwrap_or_else(EventHandler::new),
            llm_client: self.llm_client.unwrap_or_else(|| Arc::new(MockLLMClient::new())),
            renderer: self.renderer.unwrap_or_else(ResponseRenderer::new),
        })
    }
}

impl InfrawareTerminal {
    pub fn builder() -> InfrawareTerminalBuilder {
        InfrawareTerminalBuilder::new()
    }
}
```

#### Uso

```rust
// Testing con componenti custom
let terminal = InfrawareTerminal::builder()
    .with_llm_client(Arc::new(MockLLMClient::new()))
    .with_classifier(custom_classifier)
    .build()?;

// Produzione da config file (M2)
let config = load_config("config.toml")?;
let terminal = InfrawareTerminal::builder()
    .with_llm_client(create_llm_from_config(&config)?)
    .with_classifier(create_classifier_from_config(&config)?)
    .build()?;
```

#### Benefici

- ✅ Testing con mock components
- ✅ Configuration file support (M2)
- ✅ Fluent, readable API
- ✅ Default values sensibili

---

### 📋 3. Strategy Pattern - Input Classification

**Status**: 📋 **PLANNED** (Fase 2 - Week 3)
**Priorità**: HIGH
**Effort**: Medium
**Impact**: Estensibilità + Plugin System

#### Problema

`InputClassifier` ha 200+ righe di logica monolitica che non può essere estesa:

```rust
// ❌ PROBLEMA: Logica hardcoded
impl InputClassifier {
    pub fn classify(&self, input: &str) -> Result<InputType> {
        // 1. Check known commands (hardcoded list)
        if self.known_commands.contains(first_word) { ... }

        // 2. Check command syntax (hardcoded patterns)
        if input.contains(" -") || input.contains(" --") { ... }

        // 3. Check natural language (hardcoded heuristics)
        if input.starts_with("how ") || input.starts_with("what ") { ... }

        // Cannot add new strategies without modifying this code!
    }
}
```

#### Soluzione Proposta

```rust
// Trait per strategie di classificazione
pub trait ClassificationStrategy: Send + Sync {
    fn classify(&self, input: &str) -> Option<InputType>;
    fn priority(&self) -> u8; // Higher = checked first
}

// Strategia: Known Commands
pub struct KnownCommandStrategy {
    known_commands: HashSet<String>,
}

impl ClassificationStrategy for KnownCommandStrategy {
    fn classify(&self, input: &str) -> Option<InputType> {
        let first_word = input.split_whitespace().next()?;
        if self.known_commands.contains(first_word) {
            let parts = shell_words::split(input).ok()?;
            Some(InputType::Command(parts[0].clone(), parts[1..].to_vec()))
        } else {
            None
        }
    }

    fn priority(&self) -> u8 { 100 } // Highest
}

// Strategia: Command Syntax
pub struct CommandSyntaxStrategy;

impl ClassificationStrategy for CommandSyntaxStrategy {
    fn classify(&self, input: &str) -> Option<InputType> {
        if input.contains(" -") || input.contains(" --")
            || input.contains('|') || input.contains('>') {
            let parts = shell_words::split(input).ok()?;
            Some(InputType::Command(parts[0].clone(), parts[1..].to_vec()))
        } else {
            None
        }
    }

    fn priority(&self) -> u8 { 80 }
}

// Strategia: Natural Language
pub struct NaturalLanguageStrategy {
    question_words: Vec<String>,
    polite_words: Vec<String>,
}

impl ClassificationStrategy for NaturalLanguageStrategy {
    fn classify(&self, input: &str) -> Option<InputType> {
        let lower = input.to_lowercase();
        for word in &self.question_words {
            if lower.starts_with(word) {
                return Some(InputType::NaturalLanguage(input.to_string()));
            }
        }
        None
    }

    fn priority(&self) -> u8 { 50 }
}

// Coordinator
pub struct InputClassifier {
    strategies: Vec<Box<dyn ClassificationStrategy>>,
}

impl InputClassifier {
    pub fn new() -> Self {
        let mut strategies: Vec<Box<dyn ClassificationStrategy>> = vec![
            Box::new(KnownCommandStrategy::default()),
            Box::new(CommandSyntaxStrategy),
            Box::new(NaturalLanguageStrategy::default()),
        ];

        // Sort by priority
        strategies.sort_by_key(|s| std::cmp::Reverse(s.priority()));

        Self { strategies }
    }

    pub fn add_strategy(&mut self, strategy: Box<dyn ClassificationStrategy>) {
        self.strategies.push(strategy);
        self.strategies.sort_by_key(|s| std::cmp::Reverse(s.priority()));
    }

    pub fn classify(&self, input: &str) -> Result<InputType> {
        for strategy in &self.strategies {
            if let Some(result) = strategy.classify(input) {
                return Ok(result);
            }
        }

        // Default fallback
        Ok(InputType::NaturalLanguage(input.to_string()))
    }
}
```

#### Benefici

- ✅ Ogni strategia è testabile isolatamente
- ✅ Facile aggiungere nuove strategie (M2/M3)
- ✅ Plugin system può registrare strategie custom
- ✅ A/B testing tra strategie diverse
- ✅ Priorità configurabili

#### File Coinvolti

- `src/input/classifier.rs` - Refactor completo
- `src/input/strategies/` - Nuovo modulo con strategie

---

### 📋 4. Strategy Pattern - Package Managers

**Status**: 📋 **PLANNED** (Fase 2 - Week 3)
**Priorità**: HIGH
**Effort**: Low-Medium
**Impact**: Codice più pulito + Estensibilità

#### Problema

`PackageInstaller` ha un grande match statement con 7+ branch:

```rust
// ❌ PROBLEMA: Match statement non estendibile
impl PackageInstaller {
    pub async fn install_package(package: &str) -> Result<()> {
        let pm = Self::detect_package_manager()?;

        match pm.as_str() {
            "apt-get" => Self::install_with_apt(package).await,
            "yum" => Self::install_with_yum(package).await,
            "dnf" => Self::install_with_dnf(package).await,
            "pacman" => Self::install_with_pacman(package).await,
            "brew" => Self::install_with_brew(package).await,
            "choco" => Self::install_with_choco(package).await,
            "winget" => Self::install_with_winget(package).await,
            _ => anyhow::bail!("Unsupported package manager"),
        }
    }
}
```

#### Soluzione Proposta

```rust
// Trait per package manager
pub trait PackageManager: Send + Sync {
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn install(&self, package: &str) -> BoxFuture<'static, Result<()>>;
    fn priority(&self) -> u8;
}

// Implementazione APT
pub struct AptPackageManager;

impl PackageManager for AptPackageManager {
    fn name(&self) -> &str { "apt-get" }

    fn is_available(&self) -> bool {
        CommandExecutor::command_exists("apt-get")
    }

    fn install(&self, package: &str) -> BoxFuture<'static, Result<()>> {
        let package = package.to_string();
        async move {
            CommandExecutor::execute_sudo("apt-get", &[
                "install".to_string(),
                "-y".to_string(),
                package,
            ]).await?;
            Ok(())
        }.boxed()
    }

    fn priority(&self) -> u8 { 80 }
}

// Registry
pub struct PackageInstaller {
    managers: Vec<Box<dyn PackageManager>>,
}

impl PackageInstaller {
    pub fn new() -> Self {
        let managers: Vec<Box<dyn PackageManager>> = vec![
            Box::new(AptPackageManager),
            Box::new(YumPackageManager),
            Box::new(BrewPackageManager),
            // ... altri
        ];

        Self { managers }
    }

    pub fn register(&mut self, manager: Box<dyn PackageManager>) {
        self.managers.push(manager);
    }

    pub async fn install_package(&self, package: &str) -> Result<()> {
        let manager = self.managers
            .iter()
            .filter(|m| m.is_available())
            .max_by_key(|m| m.priority())
            .ok_or_else(|| anyhow::anyhow!("No package manager available"))?;

        manager.install(package).await
    }
}
```

#### Benefici

- ✅ Aggiungere package manager = implementare trait
- ✅ Plugin system può registrare custom managers
- ✅ Priority system per preferenze
- ✅ Testabile isolatamente

---

### 📋 5. Command Pattern - Event Handling

**Status**: 📋 **PLANNED** (Fase 3 - M2)
**Priorità**: MEDIUM-HIGH
**Effort**: Medium
**Impact**: Testabilità + Features avanzate

#### Problema

Event handling è un grande match con logica embedded:

```rust
// ❌ PROBLEMA: Logica non riutilizzabile
async fn handle_event(&mut self, event: TerminalEvent) -> Result<bool> {
    match event {
        TerminalEvent::Submit => {
            // 50+ lines di logica qui dentro
            let input = self.state.submit_input();
            // ... complessa logica di classificazione
            // ... esecuzione comando
            // ... query LLM
        }
        TerminalEvent::TabComplete => {
            // 30+ lines di logica qui dentro
        }
        // ... 14 branches totali
    }
}
```

#### Soluzione Proposta

```rust
// Command trait
pub trait TerminalCommand: Send + Sync {
    fn execute(
        &self,
        state: &mut TerminalState,
        context: &mut CommandContext,
    ) -> BoxFuture<'_, Result<CommandResult>>;

    fn can_undo(&self) -> bool { false }
    fn undo(&self, state: &mut TerminalState) -> Result<()> { Ok(()) }
}

pub struct CommandContext {
    pub ui: Arc<TerminalUI>,
    pub classifier: Arc<InputClassifier>,
    pub llm_client: Arc<dyn LLMClientTrait>,
    pub renderer: Arc<ResponseRenderer>,
}

// Submit command
pub struct SubmitCommand {
    classifier: Arc<InputClassifier>,
    llm_client: Arc<dyn LLMClientTrait>,
}

impl TerminalCommand for SubmitCommand {
    fn execute(&self, state: &mut TerminalState, ctx: &mut CommandContext)
        -> BoxFuture<'_, Result<CommandResult>>
    {
        async move {
            let input = state.submit_input();
            // ... logica submit
            Ok(CommandResult::Continue)
        }.boxed()
    }
}

// Input char command (with undo)
pub struct InputCharCommand(char);

impl TerminalCommand for InputCharCommand {
    fn execute(&self, state: &mut TerminalState, _: &mut CommandContext)
        -> BoxFuture<'_, Result<CommandResult>>
    {
        async move {
            state.insert_char(self.0);
            Ok(CommandResult::Continue)
        }.boxed()
    }

    fn can_undo(&self) -> bool { true }

    fn undo(&self, state: &mut TerminalState) -> Result<()> {
        state.delete_char();
        Ok(())
    }
}

// Factory
pub struct CommandFactory;

impl CommandFactory {
    pub fn create(event: TerminalEvent, ctx: &CommandContext)
        -> Box<dyn TerminalCommand>
    {
        match event {
            TerminalEvent::Submit => Box::new(SubmitCommand { ... }),
            TerminalEvent::InputChar(c) => Box::new(InputCharCommand(c)),
            // ...
        }
    }
}

// Simplified handler
async fn handle_event(&mut self, event: TerminalEvent) -> Result<bool> {
    let command = CommandFactory::create(event, &self.context);
    let result = command.execute(&mut self.state, &mut self.context).await?;

    match result {
        CommandResult::Continue => Ok(true),
        CommandResult::Quit => Ok(false),
    }
}
```

#### Benefici

- ✅ Ogni comando testabile isolatamente
- ✅ Undo/Redo support
- ✅ Macro recording (M3)
- ✅ Command logging per telemetry

---

### 📋 6. Chain of Responsibility - Rendering

**Status**: 📋 **PLANNED** (Fase 3 - M2)
**Priorità**: MEDIUM
**Effort**: Medium
**Impact**: Markdown avanzato (M2)

#### Problema

`ResponseRenderer` ha logica monolitica per markdown:

```rust
// ❌ PROBLEMA: Un solo metodo fa tutto
pub fn render(&self, text: &str) -> Vec<String> {
    // Parse code blocks
    // Format inline
    // Tutto insieme, difficile estendere
}
```

#### Soluzione Proposta

```rust
// Handler trait
pub trait RenderHandler: Send + Sync {
    fn can_handle(&self, line: &str, context: &RenderContext) -> bool;
    fn handle(&self, line: &str, context: &mut RenderContext) -> Vec<String>;
    fn priority(&self) -> u8;
}

pub struct RenderContext {
    pub in_code_block: bool,
    pub code_lang: String,
    pub code_lines: Vec<String>,
    // Future: table_state, list_state, etc.
}

// Code block handler
pub struct CodeBlockHandler {
    syntax_set: Arc<SyntaxSet>,
}

impl RenderHandler for CodeBlockHandler {
    fn can_handle(&self, line: &str, _: &RenderContext) -> bool {
        line.starts_with("```")
    }

    fn handle(&self, line: &str, ctx: &mut RenderContext) -> Vec<String> {
        if ctx.in_code_block {
            // End block, highlight code
            let highlighted = self.highlight(&ctx.code_lines, &ctx.code_lang);
            ctx.code_lines.clear();
            ctx.in_code_block = false;
            highlighted
        } else {
            // Start block
            ctx.code_lang = line.trim_start_matches("```").to_string();
            ctx.in_code_block = true;
            vec![]
        }
    }

    fn priority(&self) -> u8 { 100 }
}

// Chain coordinator
pub struct ResponseRenderer {
    handlers: Vec<Box<dyn RenderHandler>>,
}

impl ResponseRenderer {
    pub fn add_handler(&mut self, handler: Box<dyn RenderHandler>) {
        self.handlers.push(handler);
        self.handlers.sort_by_key(|h| std::cmp::Reverse(h.priority()));
    }

    pub fn render(&self, text: &str) -> Vec<String> {
        let mut output = Vec::new();
        let mut context = RenderContext::default();

        for line in text.lines() {
            for handler in &self.handlers {
                if handler.can_handle(line, &context) {
                    output.extend(handler.handle(line, &mut context));
                    break;
                }
            }
        }

        output
    }
}
```

#### Benefici M2

- ✅ `TableHandler` per tabelle
- ✅ `ImageHandler` per immagini
- ✅ `ListHandler` per liste
- ✅ Plugin system per custom renderers

---

### 📋 7. Observer Pattern - State Changes

**Status**: 📋 **PLANNED** (Fase 3 - M2)
**Priorità**: LOW-MEDIUM
**Effort**: Medium
**Impact**: Telemetry (M2)

#### Problema

Cambiamenti di stato sono silenziosi:

```rust
// ❌ PROBLEMA: No notifications
pub fn add_output(&mut self, line: String) {
    self.output_buffer.push(line);
    // Nessuno sa che è cambiato
}
```

#### Soluzione Proposta

```rust
// Observer trait
pub trait StateObserver: Send + Sync {
    fn on_output_added(&self, line: &str);
    fn on_command_submitted(&self, command: &str);
    fn on_mode_changed(&self, old: TerminalMode, new: TerminalMode);
}

// Telemetry observer
pub struct TelemetryObserver {
    backend: TelemetryBackend,
}

impl StateObserver for TelemetryObserver {
    fn on_command_submitted(&self, command: &str) {
        self.backend.track_event("command_executed", command);
    }
}

// Observable state
pub struct TerminalState {
    observers: Vec<Arc<dyn StateObserver>>,
    // ... fields
}

impl TerminalState {
    pub fn add_observer(&mut self, observer: Arc<dyn StateObserver>) {
        self.observers.push(observer);
    }

    pub fn add_output(&mut self, line: String) {
        // Notify observers
        for observer in &self.observers {
            observer.on_output_added(&line);
        }

        self.output_buffer.push(line);
    }
}
```

#### Benefici M2

- ✅ Telemetry and analytics
- ✅ Logging
- ✅ Debugging tools
- ✅ Plugin notifications

---

### 📋 8. Facade Pattern - Command Execution

**Status**: 📋 **PLANNED** (Fase 2 - Week 4)
**Priorità**: LOW
**Effort**: Low
**Impact**: Code cleanup

#### Problema

Command execution ripete logica:

```rust
// ❌ PROBLEMA: Duplicazione
async fn handle_command(&mut self, cmd: &str, args: &[String]) -> Result<()> {
    if !CommandExecutor::command_exists(cmd) {
        // Handle error
    }

    match CommandExecutor::execute(cmd, args).await {
        Ok(output) => {
            // Format stdout
            // Format stderr
            // Check exit code
        }
        Err(e) => {
            // Handle error
        }
    }
}
```

#### Soluzione Proposta

```rust
pub struct CommandExecutionFacade {
    executor: CommandExecutor,
    installer: PackageInstaller,
}

pub enum ExecutionResult {
    Success(CommandOutput),
    CommandNotFound { command: String, can_install: bool },
    Error(String),
}

impl CommandExecutionFacade {
    pub async fn execute_with_fallback(
        &self,
        cmd: &str,
        args: &[String],
    ) -> Result<ExecutionResult> {
        if !CommandExecutor::command_exists(cmd) {
            return Ok(ExecutionResult::CommandNotFound {
                command: cmd.to_string(),
                can_install: self.installer.is_available(),
            });
        }

        match CommandExecutor::execute(cmd, args).await {
            Ok(output) => Ok(ExecutionResult::Success(output)),
            Err(e) => Ok(ExecutionResult::Error(e.to_string())),
        }
    }
}
```

---

## Roadmap di Implementazione

### Fase 1 - Critici (Week 2) ✅

| Pattern | Status | Priority | Effort |
|---------|--------|----------|--------|
| Trait Object LLM | ✅ DONE | CRITICAL | Low |
| Builder Pattern | 📋 TODO | HIGH | Low-Medium |

### Fase 2 - Alta Priorità (Week 3-4)

| Pattern | Status | Priority | Effort |
|---------|--------|----------|--------|
| Strategy - Classification | 📋 TODO | HIGH | Medium |
| Strategy - Package Managers | 📋 TODO | HIGH | Low-Medium |
| Facade - Command Execution | 📋 TODO | LOW | Low |

### Fase 3 - Media Priorità (M2)

| Pattern | Status | Priority | Effort |
|---------|--------|----------|--------|
| Command - Event Handling | 📋 TODO | MEDIUM-HIGH | Medium |
| Chain - Rendering | 📋 TODO | MEDIUM | Medium |
| Observer - State | 📋 TODO | LOW-MEDIUM | Medium |

---

## Guide Pratiche

### Come Estendere: Aggiungere un Nuovo LLM Provider

```rust
// 1. Implementa il trait
pub struct OpenAIClient {
    api_key: String,
    model: String,
}

#[async_trait]
impl LLMClientTrait for OpenAIClient {
    async fn query(&self, text: &str) -> Result<String> {
        // Implementa logica OpenAI
        Ok(response)
    }
}

// 2. Usa in main
let client = Arc::new(OpenAIClient::new(api_key, "gpt-4"));
let terminal = InfrawareTerminal::new_with_client(client)?;
```

### Best Practices Rust

#### 1. Async Trait Methods

```rust
// ❌ Non compila
pub trait MyTrait {
    async fn my_method(&self) -> Result<String>;
}

// ✅ Usa async_trait
#[async_trait]
pub trait MyTrait {
    async fn my_method(&self) -> Result<String>;
}

// ✅ Oppure BoxFuture
pub trait MyTrait {
    fn my_method(&self) -> BoxFuture<'_, Result<String>>;
}
```

#### 2. Trait Objects

```rust
// Per trait objects, servono Send + Sync se usati tra thread
pub trait MyTrait: Send + Sync {
    fn method(&self);
}

// Usa Arc per shared ownership
let obj: Arc<dyn MyTrait> = Arc::new(MyImpl);
```

#### 3. Default Implementations

```rust
pub trait MyTrait {
    fn required_method(&self) -> String;

    // Default con self-call
    fn optional_method(&self) -> String {
        format!("Default: {}", self.required_method())
    }
}
```

### Testing Strategy

```rust
// Test con mock
#[tokio::test]
async fn test_with_mock_llm() {
    let terminal = InfrawareTerminal::builder()
        .with_llm_client(Arc::new(MockLLMClient::new()))
        .build()
        .unwrap();

    // Test logic
}

// Test con custom implementation
struct TestLLMClient {
    responses: HashMap<String, String>,
}

#[async_trait]
impl LLMClientTrait for TestLLMClient {
    async fn query(&self, text: &str) -> Result<String> {
        Ok(self.responses.get(text).cloned().unwrap_or_default())
    }
}
```

---

## Appendici

### Glossario

| Termine | Descrizione |
|---------|-------------|
| **Trait** | Interface in Rust |
| **Trait Object** | `dyn Trait` - runtime polymorphism |
| **Arc** | Atomic Reference Counted - shared ownership thread-safe |
| **Box** | Heap allocation - owned pointer |
| **BoxFuture** | Boxed future per async in traits |
| **Send** | Trait marker per thread safety (can send between threads) |
| **Sync** | Trait marker per thread safety (can share reference between threads) |

### Riferimenti SOLID

#### Single Responsibility Principle (SRP)

```rust
// ❌ Viola SRP
struct Terminal {
    fn render() {}
    fn execute_command() {}
    fn query_llm() {}
    fn manage_state() {}
}

// ✅ Rispetta SRP
struct TerminalUI { fn render() {} }
struct CommandExecutor { fn execute() {} }
struct LLMClient { fn query() {} }
struct TerminalState { fn manage() {} }
```

#### Open/Closed Principle (OCP)

```rust
// ❌ Viola OCP - devi modificare per estendere
match package_manager {
    "apt" => install_apt(),
    "yum" => install_yum(),
    // Devi aggiungere case qui per nuovi PM
}

// ✅ Rispetta OCP - estendi senza modificare
trait PackageManager {
    fn install(&self, pkg: &str);
}

let managers: Vec<Box<dyn PackageManager>> = vec![
    Box::new(AptManager),
    Box::new(YumManager),
    // Aggiungi nuovi senza modificare codice esistente
];
```

#### Liskov Substitution Principle (LSP)

```rust
// ✅ Rispetta LSP - tutte le implementazioni sono intercambiabili
fn use_llm(client: Arc<dyn LLMClientTrait>) {
    client.query("test").await; // Funziona con qualsiasi implementazione
}

use_llm(Arc::new(MockLLMClient::new()));
use_llm(Arc::new(HttpLLMClient::new(url)));
use_llm(Arc::new(OpenAIClient::new(key)));
```

#### Dependency Inversion Principle (DIP)

```rust
// ❌ Viola DIP - dipende da implementazione concreta
struct Terminal {
    llm: MockLLMClient,  // Dipendenza su implementazione
}

// ✅ Rispetta DIP - dipende da astrazione
struct Terminal {
    llm: Arc<dyn LLMClientTrait>,  // Dipendenza su trait
}
```

### Pattern Relationships

```
Dependency Injection (DIP)
    │
    ├─ Trait Object Pattern (LLM Client)
    │   └─ Usa: Arc<dyn Trait>
    │
    ├─ Builder Pattern (InfrawareTerminal)
    │   └─ Facilita: Injection di dipendenze
    │
    └─ Strategy Pattern (Classification, Package Managers)
        └─ Usa: Vec<Box<dyn Trait>>

Command Pattern (Event Handling)
    │
    └─ Usa: Dependency Injection per context

Chain of Responsibility (Rendering)
    │
    └─ Simile a: Strategy Pattern
    └─ Usa: Priority + Context passing

Observer Pattern (State Changes)
    │
    └─ Usa: Vec<Arc<dyn Trait>>
```

### Migration Checklist

Quando implementi un nuovo pattern:

- [ ] Definisci il trait
- [ ] Implementa almeno 2 implementazioni concrete
- [ ] Scrivi unit test per ogni implementazione
- [ ] Scrivi integration test
- [ ] Aggiorna documentazione
- [ ] Aggiorna CLAUDE.md se necessario
- [ ] Run `cargo clippy`
- [ ] Run `cargo test`
- [ ] Commit con messaggio descrittivo

---

**Fine Documento**

*Versione 1.0 - Week 2 M1*
*Ultima modifica: Dopo implementazione Trait Object Pattern*
