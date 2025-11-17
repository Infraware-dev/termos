# Chain of Responsibility Pattern - Input Classification

## Overview

The input classification system uses the **Chain of Responsibility** design pattern to determine whether user input is a shell command or natural language query.

## Pattern Implementation

### Structure

```
User Input → ClassifierChain → [Handler 1] → [Handler 2] → ... → [Handler N]
                                     ↓
                              InputType Result
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
- Return `None` to pass the input to the next handler

#### 2. **ClassifierChain**
Coordinates the chain of handlers and processes input sequentially.

```rust
pub struct ClassifierChain {
    handlers: Vec<Box<dyn InputHandler>>,
}
```

#### 3. **Concrete Handlers**

The default chain consists of 5 handlers, executed in order:

##### a. **EmptyInputHandler**
- **Responsibility**: Detect empty or whitespace-only input
- **Returns**: `InputType::Empty`
- **Position**: First in chain

##### b. **KnownCommandHandler**
- **Responsibility**: Check against whitelist of known DevOps commands
- **Contains**: 60+ common commands (docker, kubectl, aws, git, etc.)
- **Returns**: `InputType::Command(cmd, args)`
- **Position**: Second in chain

##### c. **CommandSyntaxHandler**
- **Responsibility**: Detect command syntax patterns
- **Detects**:
  - Flags (`-` or `--`)
  - Pipes and redirects (`|`, `>`, `<`)
  - Environment variables (`$`, `${`)
  - File paths (`/`, `./`, `../`)
  - Single-word inputs (potential commands)
- **Returns**: `InputType::Command(cmd, args)`
- **Position**: Third in chain

##### d. **NaturalLanguageHandler**
- **Responsibility**: Detect natural language patterns
- **Supports**: English, Italian, Spanish, French, German
- **Detects**:
  - Question marks (`?`, `¿`)
  - Question words (how, what, come, qué, comment, wie, etc.)
  - Articles (the, il, el, le, der, etc.)
  - Request verbs (show, explain, mostrami, muéstrame, etc.)
  - Polite expressions (please, per favore, por favor, bitte, etc.)
- **Returns**: `InputType::NaturalLanguage(query)`
- **Position**: Fourth in chain

##### e. **DefaultHandler**
- **Responsibility**: Fallback for ambiguous input
- **Returns**: `InputType::NaturalLanguage(query)` (default behavior)
- **Position**: Last in chain (always handles input)

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

### Handler Order Rationale

1. **EmptyInputHandler first**: Fast fail for empty input
2. **KnownCommandHandler second**: Fastest classification (hash lookup)
3. **CommandSyntaxHandler third**: Syntax checks before NL heuristics
4. **NaturalLanguageHandler fourth**: More complex pattern matching
5. **DefaultHandler last**: Safety net (always handles input)

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

## References

- **Pattern**: Chain of Responsibility (GoF)
- **Implementation**: `src/input/handler.rs`
- **Tests**: `src/input/handler.rs` (tests module)
- **Usage**: `src/input/classifier.rs`
