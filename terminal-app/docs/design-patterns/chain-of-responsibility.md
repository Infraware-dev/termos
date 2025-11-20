# Chain of Responsibility Pattern - SCAN Algorithm

## Overview

The **SCAN Algorithm** (Shell-Command And Natural-language) implements the **Chain of Responsibility** design pattern to classify user input with high performance (<100μs average). This is the core of Infraware Terminal's input classification system.

**Status**: ✅ Production-Ready (M1 Complete)
**Performance**: <100μs average classification
**Test Coverage**: 229 tests passing

## Pattern Implementation

### SCAN Architecture

```
User Input → InputClassifier → 8-Handler Chain (Strict Order)
                                       ↓
        ┌────────────────────────────┼────────────────────────────┐
        ↓                            ↓                            ↓
    Command                      Typo?                   Natural Language
        ↓                            ↓                            ↓
  Shell Exec                   Suggestion                   LLM Backend
```

### Components

#### 1. **InputHandler Trait**
Located in `src/input/handler.rs`

```rust
pub trait InputHandler: Send + Sync {
    fn handle(&self, input: &str) -> Option<InputType>;
    fn name(&self) -> &str;
}
```

Defines the interface for all handlers in the chain. Each handler can either:
- Return `Some(InputType)` if it can classify the input
- Return `None` to pass the input to the next handler in the chain

**Key Constraint**: Handlers execute in strict order - order matters for performance!

#### 2. **InputClassifier**
Coordinates the 8-handler chain and processes input sequentially.

```rust
pub struct InputClassifier {
    chain: ClassifierChain,
}

impl InputClassifier {
    pub fn new() -> Self {
        // Creates default 7-handler chain in optimal order
    }

    pub fn classify(&self, input: &str) -> Result<InputType> {
        self.chain.process(input)
            .ok_or_else(|| anyhow::anyhow!("Failed to classify input"))
    }
}
```

#### 3. **7 Concrete Handlers (SCAN Chain)**

The production chain consists of **7 handlers**, executed in strict order for optimal performance:

##### a. **EmptyInputHandler** (~<1μs)
- **Responsibility**: Fast path for empty/whitespace input
- **Logic**: `input.trim().is_empty()`
- **Returns**: `InputType::Empty`
- **Position**: First in chain (fastest check)
- **Performance**: <1μs

##### b. **PathCommandHandler** (~10μs)
- **Responsibility**: Detect executable paths (unambiguous command intent)
- **Detects**: Paths starting with `/`, `./`, `../`
- **Platform-specific**:
  - Unix: Checks executable bit (`mode & 0o111`)
  - Windows: Checks extensions (.exe, .bat, .cmd, .ps1, .sh)
- **Returns**: `InputType::Command { command, args, original_input }`
- **Position**: Second in chain
- **Performance**: ~10μs (file system check)

##### c. **KnownCommandHandler** (<1μs cache hit, 1-5ms cache miss)
- **Responsibility**: Whitelist of 60+ DevOps commands + PATH verification
- **Contains**: docker, kubectl, aws, git, terraform, helm, ansible, etc.
- **Caching**: Thread-safe `RwLock<CommandCache>` for PATH lookups
- **Returns**: `InputType::Command { command, args, original_input }`
- **Position**: Third in chain (70% of inputs hit this handler)
- **Performance**: <1μs cache hit, 1-5ms cache miss (subsequent calls cached)

##### d. **CommandSyntaxHandler** (~10μs)
- **Responsibility**: Detect shell syntax patterns
- **Detects**:
  - Flags (`-` or `--`)
  - Pipes (`|`)
  - Redirects (`>`, `<`, `>>`, `2>`)
  - Logical operators (`&&`, `||`, `;`)
  - Environment variables (`$VAR`, `${VAR}`)
  - Subshells (`$(...)`, backticks)
  - File paths (`/`, `./`, `../`)
- **Returns**: `InputType::Command { command, args, original_input: Some(...) }`
- **Position**: Fourth in chain
- **Performance**: ~10μs (precompiled regex patterns)
- **Note**: Preserves `original_input` for shell interpretation (`sh -c`)

##### e. **TypoDetectionHandler** (~100μs)
- **Responsibility**: Detect command typos using Levenshtein distance
- **Algorithm**: Levenshtein distance ≤2 against known commands
- **Heuristic**: Only checks inputs that "look like commands" (≤5 words, no `?`, no articles)
- **Returns**: `InputType::CommandTypo { input, suggestion, distance }`
- **Position**: Fifth in chain (before LLM to prevent expensive API calls)
- **Performance**: ~100μs (60 distance calculations)
- **Example**: "dokcer ps" → suggests "docker" (distance=2)
- **Cost Savings**: Prevents LLM call ($0.001-$0.01) with 1000x faster local check

##### f. **NaturalLanguageHandler** (~5μs)
- **Responsibility**: Detect English natural language patterns
- **Language Support**: English-only patterns (multilingual delegated to LLM)
- **Detects** (using precompiled regex):
  - Question words: how, what, why, when, where, who, which
  - Polite phrases: can you, could you, please, help, show me, explain
  - Articles: a, an, the
  - Punctuation: `?`, `!`
  - Long phrases: >5 words without command syntax
- **Returns**: `InputType::NaturalLanguage(query)`
- **Position**: Sixth in chain
- **Performance**: ~5μs (precompiled `RegexSet` via `once_cell::Lazy`)
- **Design Rationale**: English-first fast path (70-80% of queries), LLM handles all other languages

##### g. **DefaultHandler** (<1μs)
- **Responsibility**: Fallback for ambiguous input (guarantees result)
- **Logic**: Always returns `InputType::NaturalLanguage(input)`
- **Returns**: `InputType::NaturalLanguage(query)`
- **Position**: Last in chain (safety net)
- **Performance**: <1μs
- **Purpose**: Ensures chain never fails - all input gets classified

## Usage

### Default Configuration

```rust
use infraware_terminal::input::InputClassifier;

let classifier = InputClassifier::new();
let result = classifier.classify("docker ps")?;
// Result: InputType::Command("docker", vec!["ps"])
```

### Custom Chain

```rust
use infraware_terminal::input::{
    InputClassifier, ClassifierChain, KnownCommandHandler,
    CommandSyntaxHandler, DefaultHandler
};

// Create custom chain with only specific handlers
let mut known_commands = KnownCommandHandler::with_defaults();
known_commands.add_command("mycmd".to_string());

let chain = ClassifierChain::new()
    .add_handler(Box::new(known_commands))
    .add_handler(Box::new(CommandSyntaxHandler::new()))
    .add_handler(Box::new(DefaultHandler::new()));

let classifier = InputClassifier::with_chain(chain);
```

## Benefits

### 1. **Single Responsibility Principle**
Each handler has one clear responsibility:
- EmptyInputHandler → empty input
- KnownCommandHandler → whitelist checking
- CommandSyntaxHandler → syntax detection
- NaturalLanguageHandler → NL pattern matching
- DefaultHandler → fallback

### 2. **Open/Closed Principle**
New handlers can be added without modifying existing code:
```rust
pub struct CustomHandler;

impl InputHandler for CustomHandler {
    fn handle(&self, input: &str) -> Option<InputType> {
        // Custom logic
    }

    fn name(&self) -> &str {
        "CustomHandler"
    }
}

// Add to chain
let chain = ClassifierChain::new()
    .add_handler(Box::new(CustomHandler))
    .add_handler(Box::new(DefaultHandler::new()));
```

### 3. **Flexibility**
- Order matters: handlers are executed sequentially
- Reusable: handlers can be used in multiple chains
- Composable: create different chains for different contexts

### 4. **Testability**
Each handler can be tested in isolation:
```rust
#[test]
fn test_known_command_handler() {
    let handler = KnownCommandHandler::with_defaults();
    assert!(matches!(
        handler.handle("docker ps"),
        Some(InputType::Command(_, _))
    ));
}
```

## Design Decisions

### Why Chain of Responsibility?

**Before (Monolithic Classifier):**
```rust
fn classify(&self, input: &str) -> Result<InputType> {
    if is_empty(input) { ... }
    else if is_known_command(input) { ... }
    else if looks_like_command(input) { ... }
    else if is_natural_language(input) { ... }
    else { ... }
}
```

**Problems:**
- Single large method with multiple responsibilities
- Hard to extend (need to modify existing code)
- Difficult to test individual classification logic
- Order of checks is implicit

**After (Chain of Responsibility):**
```rust
impl ClassifierChain {
    pub fn process(&self, input: &str) -> Option<InputType> {
        for handler in &self.handlers {
            if let Some(result) = handler.handle(input) {
                return Some(result);
            }
        }
        None
    }
}
```

**Benefits:**
- Clear separation of concerns
- Easy to add/remove handlers
- Each handler is independently testable
- Explicit order via chain construction

### Handler Order Rationale - Performance Optimized

The handler order is optimized for **real-world usage patterns** and **performance**:

1. **EmptyInputHandler first** (<1μs): Fast fail for empty input - handles ~2% of cases
2. **PathCommandHandler second** (~10μs): Unambiguous executable paths - handles ~1% of cases
3. **KnownCommandHandler third** (<1μs cache): **70% of inputs hit here** - fast cached PATH lookup
4. **CommandSyntaxHandler fourth** (~10μs): Shell syntax detection - handles ~5% of cases
5. **TypoDetectionHandler fifth** (~100μs): Prevents expensive LLM calls - handles ~3% of cases
6. **NaturalLanguageHandler sixth** (~5μs): English patterns - handles ~15% of cases
7. **DefaultHandler last** (<1μs): Safety net for remaining ~4% - always succeeds

**Average Classification Time**: ~10μs (dominated by KnownCommandHandler cache hits)

**Performance Distribution**:
```
Handler                  Avg Time    Hit Rate
────────────────────────────────────────────
EmptyInputHandler        <1μs        ~2%
PathCommandHandler       ~10μs       ~1%
KnownCommandHandler      <1μs        ~70%  ← MOST COMMON
CommandSyntaxHandler     ~10μs       ~5%
TypoDetectionHandler     ~100μs      ~3%
NaturalLanguageHandler   ~5μs        ~15%
DefaultHandler           <1μs        ~4%
────────────────────────────────────────────
TOTAL (weighted avg)     ~10μs       100%
```

## Future Enhancements (M2/M3)

### 1. Configurable Chain from File
```toml
# config.toml
[classifier]
handlers = [
    "empty",
    "known_command",
    "custom_regex",  # New handler
    "command_syntax",
    "natural_language",
    "default"
]
```

### 2. Handler Metrics
```rust
pub struct MetricsHandler<H: InputHandler> {
    inner: H,
    metrics: Arc<Mutex<HandlerMetrics>>,
}

impl<H: InputHandler> InputHandler for MetricsHandler<H> {
    fn handle(&self, input: &str) -> Option<InputType> {
        let start = Instant::now();
        let result = self.inner.handle(input);
        self.metrics.lock().unwrap().record(start.elapsed());
        result
    }
}
```

### 3. Context-Aware Handlers
```rust
pub trait ContextAwareHandler: Send + Sync {
    fn handle(&self, input: &str, context: &Context) -> Option<InputType>;
}

pub struct Context {
    pub current_directory: PathBuf,
    pub environment_vars: HashMap<String, String>,
    pub command_history: Vec<String>,
}
```

## Performance Optimizations

### 1. Precompiled RegexSet Patterns
**File**: `src/input/patterns.rs`

All regex patterns are compiled once at startup using `once_cell::Lazy`:
```rust
static PATTERNS: Lazy<CompiledPatterns> = Lazy::new(|| {
    CompiledPatterns {
        command_syntax: RegexSet::new([...]).unwrap(),
        natural_language: RegexSet::new([...]).unwrap(),
    }
});
```

**Benefit**: 10-100x faster than runtime compilation

### 2. Thread-Safe Command Cache
**File**: `src/input/discovery.rs`

PATH lookups are cached using `RwLock<CommandCache>`:
```rust
static COMMAND_CACHE: Lazy<RwLock<CommandCache>> = Lazy::new(|| {
    RwLock::new(CommandCache {
        available: HashSet::new(),
        unavailable: HashSet::new(),
    })
});
```

**Performance**:
- Cache hit: <1μs (hash lookup)
- Cache miss: 1-5ms (PATH search via `which` crate)
- Read-heavy: 99% reads, 1% writes

### 3. Lazy Initialization
All expensive resources use `once_cell::Lazy` for zero-cost abstraction:
- Regex patterns compiled once
- Command cache initialized on first use
- No startup penalty

## References

- **Pattern**: Chain of Responsibility (GoF Design Pattern)
- **Algorithm**: SCAN (Shell-Command And Natural-language)
- **Implementation Files**:
  - `src/input/classifier.rs` - Main coordinator
  - `src/input/handler.rs` - 7 handler implementations (987 lines)
  - `src/input/patterns.rs` - Precompiled patterns (258 lines)
  - `src/input/discovery.rs` - Command cache (414 lines)
  - `src/input/typo_detection.rs` - Levenshtein distance (465 lines)
- **Tests**: `tests/classifier_tests.rs` - 229 tests passing
- **Benchmarks**: `benches/scan_benchmark.rs` - Performance benchmarks
- **Documentation**: `docs/SCAN_ARCHITECTURE.md` - Complete SCAN documentation (963 lines)

## Status

**✅ Production-Ready (M1 Complete)**
- All 8 handlers implemented and tested
- Performance targets achieved (<100μs average)
- Zero clippy warnings
- 229 tests passing
- 75% code coverage enforced by CI
- Cross-platform support (Ubuntu, Windows, macOS)
