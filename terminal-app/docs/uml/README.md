# Infraware Terminal - UML Class Diagrams

Questa cartella contiene i diagrammi UML delle classi del progetto Infraware Terminal, generati in formato PlantUML.

## 📁 Struttura dei Diagrammi

### Diagramma Completo
- **`class-diagram.puml`** - Diagramma UML completo di tutto il sistema con tutte le classi, relazioni e design pattern evidenziati

### Diagrammi Modulari

I diagrammi sono stati organizzati per modulo per migliorare la leggibilità:

1. **`main-application.puml`** - Applicazione principale e Builder Pattern
2. **`orchestrators.puml`** - Orchestrator Pattern per gestione workflow
3. **`executor.puml`** - Facade e Strategy Pattern per esecuzione comandi
4. **`input.puml`** - Chain of Responsibility per classificazione input
5. **`llm.puml`** - Strategy Pattern per client LLM e rendering
6. **`terminal.puml`** - Terminal UI, state management ed event handling
7. **`utils.puml`** - Utilities (message formatting, ANSI, errors)

## 🎨 Legenda Colori

I design pattern sono evidenziati con colori specifici:

| Pattern | Colore | Esempio |
|---------|--------|---------|
| 🔨 **Builder** | Giallo chiaro (`#FFFACD`) | InfrawareTerminalBuilder |
| 🎯 **Strategy** | Verde chiaro (`#90EE90`) | PackageManager, LLMClientTrait |
| ⛓️ **Chain of Responsibility** | Azzurro (`#87CEEB`) | InputHandler chain |
| 🎭 **Facade** | Viola chiaro (`#DDA0DD`) | CommandExecutionFacade |
| 🎼 **Orchestrator** | Arancione (`#FFB366`) | CommandOrchestrator, etc. |

## 🛠️ Come Visualizzare i Diagrammi

### Opzione 1: Online (Più Semplice)

1. Visita [PlantUML Online Editor](http://www.plantuml.com/plantuml/uml/)
2. Copia il contenuto di uno dei file `.puml`
3. Incolla nell'editor online
4. Il diagramma verrà renderizzato automaticamente

### Opzione 2: VS Code Extension

1. Installa l'estensione [PlantUML](https://marketplace.visualstudio.com/items?itemName=jebbs.plantuml)
2. Installa Java (richiesto da PlantUML)
3. Apri un file `.puml` in VS Code
4. Premi `Alt+D` per preview

### Opzione 3: IntelliJ IDEA / PyCharm

1. Installa il plugin [PlantUML Integration](https://plugins.jetbrains.com/plugin/7017-plantuml-integration)
2. Apri un file `.puml`
3. La preview appare automaticamente nel pannello laterale

### Opzione 4: Command Line

```bash
# Installa PlantUML
brew install plantuml  # macOS
# oppure scarica da https://plantuml.com/download

# Genera immagini PNG
plantuml docs/uml/*.puml

# Genera SVG (scalabile)
plantuml -tsvg docs/uml/*.puml

# Output in docs/uml/ con estensione .png o .svg
```

### Opzione 5: Docker

```bash
docker run --rm -v $(pwd):/data plantuml/plantuml \
  -tsvg "docs/uml/*.puml"
```

## 📚 Design Pattern Utilizzati

### 1. Builder Pattern 🔨

**File**: `main-application.puml`
**Classe**: `InfrawareTerminalBuilder`

**Scopo**: Costruzione flessibile di `InfrawareTerminal` con dependency injection.

**Vantaggi**:
- Configurazione opzionale dei componenti
- Testabilità (mock injection)
- Default sensibili per sviluppo rapido

**Esempio**:
```rust
InfrawareTerminal::builder()
    .with_llm_client(Arc::new(HttpLLMClient::new(url)))
    .build()?
```

### 2. Strategy Pattern 🎯

**File**: `executor.puml`, `llm.puml`
**Interfacce**: `PackageManager`, `LLMClientTrait`

**Scopo**: Intercambiabilità di algoritmi a runtime.

**Implementazioni PackageManager**:
- Linux: `AptPackageManager`, `YumPackageManager`, `DnfPackageManager`, `PacmanPackageManager`
- macOS: `BrewPackageManager`
- Windows: `ChocoPackageManager`, `WingetPackageManager`

**Implementazioni LLMClient**:
- `HttpLLMClient` - Produzione (REST API)
- `MockLLMClient` - Development/Testing

### 3. Chain of Responsibility ⛓️

**File**: `input.puml`
**Interfaccia**: `InputHandler`

**Scopo**: Classificazione input utente attraverso una catena di handler.

**Catena di Handler** (ordine importante):
1. `EmptyInputHandler` - Gestisce input vuoto
2. `KnownCommandHandler` - Whitelist comandi noti (fast path)
3. `CommandSyntaxHandler` - Euristica sintattica
4. `NaturalLanguageHandler` - Pattern multilingua
5. `DefaultHandler` - Fallback a natural language

**Caratteristiche**:
- Supporto multilingua (EN, IT, ES, FR, DE)
- Estensibile (aggiungi nuovi handler)
- Primo handler che gestisce vince

### 4. Facade Pattern 🎭

**File**: `executor.puml`
**Classe**: `CommandExecutionFacade`

**Scopo**: Semplificare interfaccia complessa di esecuzione comandi.

**Nasconde**:
- `CommandExecutor` - Esecuzione low-level
- `PackageInstaller` - Gestione installazione
- Logica di fallback e retry

**Fornisce**:
- `execute_with_fallback()` - Auto-retry
- `execute_or_install()` - Auto-install se comando mancante

### 5. Orchestrator Pattern 🎼

**File**: `orchestrators.puml`
**Classi**: `CommandOrchestrator`, `NaturalLanguageOrchestrator`, `TabCompletionHandler`

**Scopo**: Separare workflow logic dalla business logic (SRP).

**Orchestratori**:
- **CommandOrchestrator**: Workflow esecuzione comandi
  - Built-in commands
  - Verifica esistenza
  - Esecuzione e formatting
  - Gestione errori

- **NaturalLanguageOrchestrator**: Workflow query LLM
  - Stato "waiting"
  - Query al LLM
  - Rendering risposta (markdown + syntax highlighting)
  - Error handling

- **TabCompletionHandler**: Workflow tab completion
  - Ottenere completamenti
  - Auto-complete singolo
  - Mostrare multipli
  - Prefisso comune

## 📊 Statistiche del Sistema

### Classi e Strutture

| Categoria | Quantità |
|-----------|----------|
| Struct/Class | 47 |
| Enum | 7 |
| Trait/Interface | 3 |
| **Totale Tipi** | **57** |

### Design Pattern

| Pattern | Occorrenze | Classi |
|---------|------------|---------|
| Strategy | 2 | PackageManager (×7), LLMClientTrait (×2) |
| Chain of Responsibility | 1 | InputHandler (×5) |
| Builder | 1 | InfrawareTerminalBuilder |
| Facade | 1 | CommandExecutionFacade |
| Orchestrator | 3 | Command, NaturalLanguage, TabCompletion |

### Moduli

| Modulo | Classi | Descrizione |
|--------|--------|-------------|
| Main | 2 | Applicazione principale + Builder |
| Orchestrators | 3 | Workflow management (SRP) |
| Executor | 11 | Command execution, package management |
| Input | 8 | Input classification (Chain) |
| LLM | 5 | LLM client + response rendering |
| Terminal | 4 | TUI, state, events |
| Utils | 5 | Formatting, ANSI, errors |

## 🔗 Relazioni Principali

### Composizione (Has-A) - Aggregazione Forte

```
InfrawareTerminal
├── TerminalUI
├── TerminalState
├── InputClassifier
│   └── ClassifierChain
│       └── Vec<Box<dyn InputHandler>>
├── EventHandler
├── CommandOrchestrator
├── NaturalLanguageOrchestrator
│   ├── Arc<dyn LLMClientTrait>
│   └── ResponseRenderer
└── TabCompletionHandler
```

### Dipendenze (Uses) - Accoppiamento Debole

```
CommandOrchestrator → CommandExecutor (static)
CommandOrchestrator → PackageInstaller (static)
NaturalLanguageOrchestrator → LLMClientTrait (via Arc)
TabCompletionHandler → TabCompletion (static)
```

### Implementazione (Is-A)

```
PackageManager
├── AptPackageManager
├── YumPackageManager
├── DnfPackageManager
├── PacmanPackageManager
├── BrewPackageManager
├── ChocoPackageManager
└── WingetPackageManager

LLMClientTrait
├── HttpLLMClient
└── MockLLMClient

InputHandler
├── EmptyInputHandler
├── KnownCommandHandler
├── CommandSyntaxHandler
├── NaturalLanguageHandler
└── DefaultHandler
```

## 💡 Note Architetturali

### Single Responsibility Principle (SRP)

Ogni classe ha una sola responsabilità:

- **InfrawareTerminal**: Event loop e coordinamento
- **CommandOrchestrator**: Workflow esecuzione comandi
- **NaturalLanguageOrchestrator**: Workflow query LLM
- **TabCompletionHandler**: Workflow tab completion
- **InputClassifier**: Classificazione input
- **CommandExecutor**: Esecuzione comandi low-level
- **TerminalUI**: Rendering TUI
- **TerminalState**: Stato terminal

### Dependency Injection

Il Builder Pattern permette DI per:
- LLM client (Mock/HTTP)
- Components (UI, State, Classifier, etc.)
- Orchestrators

Questo facilita:
- Unit testing con mock
- Configurazione flessibile
- Switching tra implementazioni

### Estensibilità

Il sistema è estensibile tramite:
- **Strategy Pattern**: Aggiungi nuovi LLM client o package manager
- **Chain of Responsibility**: Aggiungi nuovi handler input
- **Builder Pattern**: Configura nuovi componenti

## 🔍 Come Leggere i Diagrammi

### Notazione UML

```
┌─────────────────────┐
│   ClassName         │ ← Nome classe
├─────────────────────┤
│ - field: Type       │ ← Campi privati (-)
│ + field: Type       │ ← Campi pubblici (+)
├─────────────────────┤
│ + method(): Type    │ ← Metodi pubblici (+)
│ - method(): Type    │ ← Metodi privati (-)
└─────────────────────┘
```

### Relazioni

- **→** (freccia tratteggiata): Dependency (uses)
- **─>** (freccia piena): Association
- **◇─**: Aggregation (has-a, weak)
- **◆─**: Composition (has-a, strong)
- **◁─**: Generalization (is-a, inheritance)
- **◁··**: Realization (implements trait)

### Stereotipi

- **<<Strategy>>**: Strategy Pattern
- **<<Chain>>**: Chain of Responsibility
- **<<Builder>>**: Builder Pattern
- **<<Facade>>**: Facade Pattern
- **<<Orchestrator>>**: Orchestrator Pattern

## 📖 Riferimenti

- **PlantUML**: https://plantuml.com/
- **UML Class Diagram**: https://plantuml.com/class-diagram
- **Project Brief**: `../infraware_terminal_project_brief.md`
- **Design Patterns Doc**: `../design-patterns.md`
- **README**: `../../README.md`

## 🆘 Supporto

Per problemi con PlantUML:
- [PlantUML Forum](https://forum.plantuml.net/)
- [PlantUML FAQ](https://plantuml.com/faq)
- [Syntax Reference](https://plantuml.com/guide)

Per domande sull'architettura del progetto:
- Vedi `CLAUDE.md` per istruzioni specifiche
- Controlla `docs/design-patterns.md` per dettagli sui pattern

---

**Generato**: 2025-11-08
**Versione**: M1 (Month 1) - Terminal Core MVP
**PlantUML Version**: 1.2024.x compatible
