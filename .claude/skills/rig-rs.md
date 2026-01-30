---
name: rig-rs
description: Apply rig-rs examples and best practices when writing Rust code that uses rig-rs (LLM orchestration framework). This includes agents, tools, embeddings, completions, extractors, vector stores, pipelines, and MCP integration.
---

# rig-rs Best Practices and Examples

Source: https://docs.rig.rs/docs/concepts

**IMPORTANT**: Always apply these patterns and best practices when writing Rust code that uses rig-rs.

---

## Installation

```bash
cargo add rig-core tokio
```

For specific features:
```bash
cargo add rig-core -F derive    # For #[derive(Embed)] macro
cargo add rig-core -F rmcp      # For MCP integration
```

---

## Provider Clients

### Creating Clients

```rust
use rig::providers::{openai, anthropic};

// From environment variable (OPENAI_API_KEY, ANTHROPIC_API_KEY)
let openai_client = openai::Client::from_env();
let anthropic_client = anthropic::Client::from_env();

// Explicit API key
let client = openai::Client::new("your-api-key");
```

### Anthropic-Specific Configuration

**CRITICAL**: Anthropic requires explicit `max_tokens` and version settings.

```rust
use rig::providers::anthropic::{ClientBuilder, CLAUDE_3_SONNET};

let client = ClientBuilder::new("your-api-key")
    .anthropic_version("2023-06-01")
    .anthropic_beta("prompt-caching-2024-07-31")  // Optional
    .build();

// max_tokens is MANDATORY for Anthropic
let agent = client
    .agent(CLAUDE_3_SONNET)
    .max_tokens(4096)  // Required!
    .preamble("You are a helpful assistant")
    .build();
```

### Available Models

**OpenAI:**
- `openai::GPT_4`, `openai::GPT_4O`
- `openai::GPT_35_TURBO`
- `openai::TEXT_EMBEDDING_3_LARGE` (3072 dims)
- `openai::TEXT_EMBEDDING_3_SMALL` (1536 dims)
- `openai::TEXT_EMBEDDING_ADA_002` (1536 dims, legacy)

**Anthropic:**
- `anthropic::CLAUDE_3_OPUS`
- `anthropic::CLAUDE_3_SONNET`
- `anthropic::CLAUDE_3_HAIKU`
- `anthropic::CLAUDE_35_SONNET`

---

## Agents

Agents combine models with context, tools, and configuration.

### Basic Agent

```rust
use rig::{providers::openai, completion::Prompt};

let openai = openai::Client::from_env();

let agent = openai
    .agent("gpt-4")
    .preamble("You are a helpful assistant.")
    .temperature(0.7)
    .build();

let response = agent.prompt("Hello!").await?;
```

### Agent with Static Context

```rust
let agent = client
    .agent("gpt-4")
    .preamble("You are a knowledge assistant.")
    .context("Important fact: The sky is blue.")
    .context("Another fact: Water is wet.")
    .build();
```

### RAG-Enabled Agent (Dynamic Context)

```rust
use rig::vector_store::InMemoryVectorStore;

let store = InMemoryVectorStore::default();
// ... add documents to store ...
let index = store.index(embedding_model);

let agent = client
    .agent("gpt-4")
    .preamble("You are a knowledge assistant.")
    .dynamic_context(3, index)  // Retrieve top 3 relevant docs
    .build();
```

### Agent with Tools

```rust
let agent = client
    .agent("gpt-4")
    .preamble("You are a capable assistant with tools.")
    .tool(Calculator)
    .tool(WebSearch)
    .max_tokens(1024)
    .build();
```

### Dynamic Tools (RAG-based tool selection)

```rust
let agent = client
    .agent("gpt-4")
    .preamble("You are a calculator.")
    .dynamic_tools(2, vector_store_index, toolset)
    .build();
```

### Multi-Turn Tool Calling

```rust
let response = agent
    .prompt("Calculate 2+5, then multiply by 3")
    .multi_turn(2)  // Allow up to 2 rounds of tool calls
    .send()
    .await?;
```

### Additional Provider Parameters

```rust
let agent = client
    .agent("gpt-4")
    .preamble("You are a helpful agent")
    .additional_params(serde_json::json!({"foo": "bar"}))
    .build();
```

---

## Completions

### Simple Prompt

```rust
use rig::completion::Prompt;

let response = model.prompt("Explain quantum computing").await?;
```

### Chat with History

```rust
use rig::completion::{Chat, Message};

let response = model
    .chat(
        "Continue the discussion",
        vec![
            Message::user("What is Rust?"),
            Message::assistant("Rust is a systems programming language..."),
        ]
    )
    .await?;
```

### Advanced Completion Request

```rust
let response = model
    .completion_request("Complex query")
    .preamble("Expert system prompt")
    .temperature(0.8)
    .max_tokens(2000)
    .documents(context_docs)
    .tools(available_tools)
    .send()
    .await?;
```

---

## Tools

### Basic Tool Implementation

```rust
use rig::tool::{Tool, ToolDefinition};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Deserialize)]
struct AddArgs {
    x: i32,
    y: i32,
}

#[derive(Debug, thiserror::Error)]
#[error("Math error: {0}")]
struct MathError(String);

#[derive(Deserialize, Serialize)]
struct Adder;

impl Tool for Adder {
    const NAME: &'static str = "add";
    type Error = MathError;
    type Args = AddArgs;
    type Output = i32;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "add".to_string(),
            description: "Add x and y together".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "x": { "type": "number", "description": "First number" },
                    "y": { "type": "number", "description": "Second number" }
                },
                "required": ["x", "y"]
            })
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(args.x + args.y)
    }
}
```

### Tool with JsonSchema (Recommended)

Use `schemars::JsonSchema` to auto-generate parameter schemas:

```rust
use schemars::{schema_for, JsonSchema};

#[derive(Deserialize, Serialize, JsonSchema)]
struct OperationArgs {
    #[schemars(description = "The first number")]
    x: i32,
    #[schemars(description = "The second number")]
    y: i32,
}

impl Tool for Calculator {
    // ...
    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "calculator".to_string(),
            description: "Perform arithmetic operations".to_string(),
            parameters: serde_json::to_value(schema_for!(OperationArgs)).unwrap(),
        }
    }
}
```

### Tool Macro (Simplest Approach)

```rust
use rig_derive::rig_tool;

#[rig_tool(
    description = "Perform basic arithmetic operations",
    required(x, y, operation)
)]
fn calculator(x: i32, y: i32, operation: String) -> Result<i32, rig::tool::ToolError> {
    match operation.as_str() {
        "add" => Ok(x + y),
        "subtract" => Ok(x - y),
        "multiply" => Ok(x * y),
        "divide" => {
            if y == 0 {
                Err(rig::tool::ToolError::ToolCallError("Division by zero".into()))
            } else {
                Ok(x / y)
            }
        }
        _ => Err(rig::tool::ToolError::ToolCallError(
            format!("Unknown operation: {operation}").into(),
        )),
    }
}
```

### RAG-Enabled Tools (ToolEmbedding)

```rust
use rig::tool::ToolEmbedding;

impl ToolEmbedding for Calculator {
    type InitError = std::convert::Infallible;
    type Context = ();
    type State = ();

    fn init(_state: Self::State, _context: Self::Context) -> Result<Self, Self::InitError> {
        Ok(Calculator)
    }

    fn embedding_docs(&self) -> Vec<String> {
        vec!["Perform arithmetic calculations like add, subtract, multiply, divide".into()]
    }

    fn context(&self) -> Self::Context {}
}
```

### Tool Server (for async contexts)

```rust
use rig::tool::server::{ToolServer, ToolServerHandle};

let tool_server: ToolServerHandle = ToolServer::new()
    .tool(Adder)
    .tool(Multiplier)
    .run();
```

---

## Extractors (Structured Data Extraction)

Extract structured data from unstructured text.

```rust
use rig::providers::openai;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct Person {
    name: Option<String>,
    age: Option<u8>,
    profession: Option<String>,
}

let openai = openai::Client::from_env();

let extractor = openai
    .extractor::<Person>("gpt-4")
    .preamble("Extract person details with high precision")
    .build();

let person = extractor
    .extract("John Doe is a 30 year old doctor.")
    .await?;

println!("Extracted: {:?}", person);
```

### Best Practices for Extractors

- Use `Option<T>` for fields that may not be present
- Keep structures focused and minimal
- Provide clear extraction instructions in preamble
- Handle `ExtractionError::NoData` when model can't extract

---

## Embeddings

### Creating Embeddings

```rust
use rig::embeddings::EmbeddingsBuilder;
use rig::providers::openai;

let client = openai::Client::from_env();
let model = client.embedding_model(openai::TEXT_EMBEDDING_3_SMALL);

let embeddings = EmbeddingsBuilder::new(model)
    .document("First document content")?
    .document("Second document content")?
    .build()
    .await?;
```

### Custom Embeddable Types

**Option 1: Derive Macro** (requires `derive` feature)

```rust
use rig::Embed;

#[derive(Embed)]
struct Document {
    id: i32,
    #[embed]
    content: String,
    #[embed]
    title: String,
}
```

**Option 2: Manual Implementation**

```rust
use rig::embeddings::{Embed, TextEmbedder, EmbedError};

struct Document {
    id: i32,
    content: String,
}

impl Embed for Document {
    fn embed(&self, embedder: &mut TextEmbedder) -> Result<(), EmbedError> {
        embedder.embed(self.content.clone());
        Ok(())
    }
}
```

### Batch Embedding

```rust
let documents = vec![
    Document { id: 1, content: "First".into() },
    Document { id: 2, content: "Second".into() },
];

let embeddings = EmbeddingsBuilder::new(model)
    .documents(documents)?
    .build()
    .await?;
```

---

## Vector Stores

### In-Memory Store (Default)

```rust
use rig::vector_store::InMemoryVectorStore;
use rig::embeddings::EmbeddingsBuilder;

// Create store
let mut store = InMemoryVectorStore::default();

// Add documents with embeddings
let embeddings = EmbeddingsBuilder::new(embedding_model)
    .simple_document("doc1", "First document content")
    .simple_document("doc2", "Second document content")
    .build()
    .await?;

store.add_documents(embeddings);

// Create index for searching
let index = store.index(embedding_model);

// Search
let results = index.top_n::<String>("search query", 5).await?;
```

### Custom Document IDs

```rust
// Auto-generated IDs
let store = InMemoryVectorStore::from_documents(vec![
    (doc1, embedding1),
    (doc2, embedding2)
]);

// Custom IDs
let store = InMemoryVectorStore::from_documents_with_ids(vec![
    ("custom_id_1", doc1, embedding1),
    ("custom_id_2", doc2, embedding2)
]);

// Function-generated IDs
let store = InMemoryVectorStore::from_documents_with_id_f(
    documents,
    |doc| format!("doc_{}", doc.id)
);
```

### Thread Safety

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

let store = Arc::new(RwLock::new(InMemoryVectorStore::default()));

// For reads
let guard = store.read().await;
let results = guard.top_n::<Doc>("query", 5).await?;

// For writes
let mut guard = store.write().await;
guard.add_documents(new_embeddings);
```

---

## Pipelines (Chains)

Build composable AI processing workflows.

### Sequential Operations

```rust
use rig::pipeline::{self, Op};

let pipeline = pipeline::new()
    .map(|(x, y)| x + y)
    .map(|z| z * 2)
    .map(|n| n.to_string());

let result = pipeline.call((5, 3)).await;
assert_eq!(result, "16");
```

### RAG Pipeline Pattern

```rust
use rig::pipeline::{self, parallel, passthrough};

let pipeline = pipeline::new()
    .chain(parallel!(
        passthrough(),
        lookup::<_, _, Document>(vector_store, 3)
    ))
    .map(|(query, docs)| {
        format!(
            "Query: {}\nContext: {}",
            query,
            docs.into_iter().map(|d| d.content).collect::<Vec<_>>().join("\n")
        )
    })
    .prompt(llm_model);

let response = pipeline.call("What is Rust?").await?;
```

### Extraction Pipeline

```rust
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
struct Sentiment {
    score: f64,
    label: String,
}

let pipeline = pipeline::new()
    .map(|text| format!("Analyze sentiment: {}", text))
    .extract::<_, _, Sentiment>(extractor);
```

### Error Handling with TryOp

```rust
let result = fallible_op
    .try_batch_call(2, vec![input1, input2])
    .await;
```

---

## Loaders

### File Loader

```rust
use rig::loaders::FileLoader;

// Load with glob pattern
let files = FileLoader::with_glob("data/*.txt")?
    .read()
    .ignore_errors()
    .into_iter();

// Load with path info
let files = FileLoader::with_glob("examples/*.rs")?
    .read_with_path()
    .ignore_errors();

for (path, content) in files {
    println!("File: {:?}", path);
}

// Load directory
let files = FileLoader::with_dir("data/")?
    .read_with_path()
    .ignore_errors();
```

### PDF Loader

```rust
use rig::loaders::PdfFileLoader;

let pages = PdfFileLoader::with_glob("docs/*.pdf")?
    .load_with_path()
    .ignore_errors()
    .by_page()
    .into_iter();
```

### Integration with Agents

```rust
let examples = FileLoader::with_glob("examples/*.rs")?
    .read_with_path()
    .ignore_errors();

let agent = examples
    .fold(client.agent("gpt-4"), |builder, (path, content)| {
        builder.context(format!("Example {:?}:\n{}", path, content).as_str())
    })
    .build();
```

---

## Model Context Protocol (MCP)

### MCP Client

```rust
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::model::{ClientInfo, ClientCapabilities, Implementation};

// Create transport
let transport = StreamableHttpClientTransport::from_uri("http://localhost:8080");

// Initialize client
let client_info = ClientInfo {
    protocol_version: Default::default(),
    capabilities: ClientCapabilities::default(),
    client_info: Implementation {
        name: "my-app".to_string(),
        version: "1.0.0".to_string(),
    },
};

let client = client_info.serve(transport).await?;

// List available tools
let tools = client.list_tools(Default::default()).await?.tools;

// Use with rig agent
let agent = openai_client
    .agent("gpt-4o")
    .rmcp_tools(tools, client.peer().to_owned())
    .build();
```

### MCP Server

```rust
use rmcp::prelude::*;

#[derive(Server)]
#[server(name = "calculator-server", version = "1.0.0")]
struct CalculatorServer;

#[server_impl]
impl CalculatorServer {
    #[tool(description = "Add two numbers together")]
    async fn add(&self, a: f64, b: f64) -> Result<f64> {
        Ok(a + b)
    }

    #[tool(description = "Multiply two numbers")]
    async fn multiply(&self, a: f64, b: f64) -> Result<f64> {
        Ok(a * b)
    }
}

// Run server
let server = CalculatorServer;
let transport = rmcp::transport::StreamableHttpServerTransport::new("127.0.0.1:8080".parse()?);
server.serve(transport).await?;
```

---

## Observability

### OpenTelemetry Integration

Rig supports OpenTelemetry GenAI Semantic Conventions for integration with backends like Langfuse and Arize Phoenix.

### OTel Collector Configuration

```yaml
receivers:
  otlp:
    protocols:
      http:
        endpoint: 0.0.0.0:4318

processors:
  transform:
    trace_statements:
      - context: span
        statements:
          - set(name, attributes["gen_ai.agent.name"]) where name == "invoke_agent"

exporters:
  otlphttp/langfuse:
    endpoint: "https://cloud.langfuse.com/api/public/otel"
    headers:
      Authorization: "Basic ${AUTH_STRING}"

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [transform]
      exporters: [otlphttp/langfuse]
```

### Custom Message-Only Logging

```rust
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

#[derive(Clone)]
struct MessageOnlyLayer;

impl<S> Layer<S> for MessageOnlyLayer
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        // Extract and log message field
        // ... implementation
    }
}

// Initialize
tracing_subscriber::registry()
    .with(EnvFilter::new("info"))
    .with(MessageOnlyLayer)
    .init();
```

---

## Error Handling Best Practices

### Tool Errors

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MyToolError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Operation failed: {0}")]
    OperationFailed(#[from] std::io::Error),
}
```

### Extraction Errors

```rust
use rig::extractor::ExtractionError;

match extractor.extract(text).await {
    Ok(data) => println!("Extracted: {:?}", data),
    Err(ExtractionError::NoData) => println!("Could not extract data"),
    Err(ExtractionError::DeserializationError(e)) => println!("Parse error: {}", e),
    Err(ExtractionError::PromptError(e)) => println!("LLM error: {}", e),
}
```

### Completion Errors

```rust
use rig::completion::PromptError;

match agent.prompt(query).await {
    Ok(response) => println!("{}", response),
    Err(PromptError::ProviderError(e)) => eprintln!("Provider error: {}", e),
    Err(e) => eprintln!("Error: {}", e),
}
```

---

## Common Patterns

### Conversational Agent

```rust
let agent = client
    .agent("gpt-4")
    .preamble("You are a friendly conversational assistant.")
    .temperature(0.9)  // Higher for more natural dialogue
    .build();
```

### RAG Knowledge Base

```rust
let agent = client
    .agent("gpt-4")
    .preamble("Answer questions based on the provided context.")
    .dynamic_context(5, document_index)  // Top 5 relevant docs
    .temperature(0.3)  // Lower for factual responses
    .build();
```

### Tool-Augmented Assistant

```rust
let agent = client
    .agent("gpt-4")
    .preamble("You are an assistant with access to tools. Use them when needed.")
    .tool(Calculator)
    .tool(WebSearch)
    .tool(FileReader)
    .max_tokens(2048)
    .build();

let response = agent
    .prompt("Search for the population of Tokyo and calculate 10% of it")
    .multi_turn(3)
    .send()
    .await?;
```

### Batch Document Processing

```rust
async fn process_documents(
    extractor: &rig::extractor::Extractor<impl rig::completion::CompletionModel, MyData>,
    docs: Vec<String>,
) -> Vec<Result<MyData, rig::extractor::ExtractionError>> {
    let mut results = Vec::new();
    for doc in docs {
        results.push(extractor.extract(&doc).await);
    }
    results
}
```

---

## Best Practices Summary

1. **Providers**
   - Use `Client::from_env()` for API keys
   - Always set `max_tokens` for Anthropic
   - Specify `anthropic_version` explicitly

2. **Agents**
   - Keep static context minimal and focused
   - Use dynamic context for large knowledge bases
   - Prefer static tools for essential functionality
   - Use `multi_turn()` for complex multi-step tasks

3. **Tools**
   - Use unique names within your application
   - Write clear descriptions for LLM understanding
   - Use `schemars::JsonSchema` for parameter schemas
   - Implement robust error handling

4. **Extractors**
   - Use `Option<T>` for optional fields
   - Provide clear extraction instructions
   - Handle `NoData` errors gracefully

5. **Embeddings**
   - Clean and normalize text before embedding
   - Use batch processing with `EmbeddingsBuilder`
   - Consider chunking for large documents

6. **Vector Stores**
   - Use in-memory store for development/testing
   - Wrap in `Arc<RwLock<>>` for concurrent access
   - Consider external stores for production scale

7. **Pipelines**
   - Design operations for modularity
   - Use `TryOp` for fallible operations
   - Leverage `parallel!` macro for concurrent execution
