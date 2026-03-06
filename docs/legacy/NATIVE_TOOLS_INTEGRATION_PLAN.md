# Piano: Integrazione Tools Nativi rig-rs

## Obiettivo

Sostituire il parsing regex/pattern con **function calling nativo** di rig-rs.

## Approccio Scelto: Tools Nativi (Option B - Manual Tool Loop)

Registriamo i tools con `.tool()` e intercettiamo `ModelChoice::ToolCall` per HITL.

---

## Scoperta Chiave

**rig-rs 0.28 NON ha il problema di type complexity!**

Il tipo `Agent<M>` rimane lo stesso indipendentemente dai tools aggiunti perché i tools sono memorizzati in `ToolSet` con dynamic dispatch (boxed).

```rust
// Entrambi producono Agent<CompletionModel>
let agent_no_tools = client.agent("model").build();
let agent_with_tools = client.agent("model")
    .tool(ShellCommandTool::new())
    .tool(AskUserTool::new())
    .build();
```

---

## Flusso con Tools Nativi

```
User: "elenca i file"
         │
         ▼
┌─────────────────────────────────────────────────────────────────┐
│ Agent con Tools (.tool(ShellCommandTool).tool(AskUserTool))     │
│                                                                 │
│ LLM vede gli schemi dei tools e decide:                        │
│ "Devo chiamare execute_shell_command"                          │
│                                                                 │
│ Output: ModelChoice::ToolCall {                                │
│   name: "execute_shell_command",                               │
│   args: { "command": "ls -la", "explanation": "..." }          │
│ }                                                               │
└─────────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────────┐
│ Orchestrator intercetta ToolCall                                │
│                                                                 │
│ match response.choice {                                        │
│   ModelChoice::ToolCall("execute_shell_command", args) => {    │
│     // HITL: salva stato, emetti interrupt                     │
│     state.store_interrupt(PendingInterrupt { ... });           │
│     yield AgentEvent::Interrupt(CommandApproval { ... });      │
│   }                                                             │
│ }                                                               │
└─────────────────────────────────────────────────────────────────┘
         │
         ▼
    [Frontend mostra: "Eseguire 'ls -la'? [y/n]"]
         │
         ▼
┌─────────────────────────────────────────────────────────────────┐
│ resume_run(Approved)                                            │
│                                                                 │
│ 1. Esegui comando con tokio::process::Command                  │
│ 2. Costruisci tool_result message                              │
│ 3. Continua conversazione con LLM includendo risultato         │
│ 4. LLM risponde con messaggio finale                           │
└─────────────────────────────────────────────────────────────────┘
```

---

## File da Modificare

| File | Modifica |
|------|----------|
| `orchestrator.rs` | Usare `.tool()`, gestire `ModelChoice::ToolCall` |
| `state.rs` | Aggiungere `tool_call_id` per tool result message |
| `tools/shell.rs` | Rimuovere `#![cfg_attr(dead_code)]` (ora usato) |
| `tools/ask_user.rs` | Rimuovere `#![cfg_attr(dead_code)]` (ora usato) |
| `config.rs` | Semplificare system prompt |

---

## Implementazione Dettagliata

### 1. orchestrator.rs - Nuovo create_agent()

```rust
use super::tools::{ShellCommandTool, AskUserTool};

pub fn create_agent_with_tools(
    client: &anthropic::Client,
    config: &RigEngineConfig,
) -> Agent<anthropic::completion::CompletionModel> {
    client
        .agent(&config.model)
        .preamble(&config.system_prompt)
        .max_tokens(config.max_tokens as u64)
        .temperature(f64::from(config.temperature))
        .tool(ShellCommandTool::new())
        .tool(AskUserTool::new())
        .build()
}
```

### 2. orchestrator.rs - Gestione ModelChoice

```rust
pub fn create_run_stream(...) -> EventStream {
    Box::pin(stream! {
        // ... setup ...

        let agent = create_agent_with_tools(&client, &config);

        // Singola completion per avere controllo su ModelChoice
        let response = agent
            .completion(&prompt, chat_history)
            .await?;

        match response.choice {
            ModelChoice::Message(text) => {
                // Risposta finale
                yield Ok(AgentEvent::Message(MessageEvent::assistant(&text)));
                yield Ok(AgentEvent::end());
            }

            ModelChoice::ToolCall(tool_name, tool_call_id, args) => {
                match tool_name.as_str() {
                    "execute_shell_command" => {
                        let shell_args: ShellCommandArgs =
                            serde_json::from_value(args)?;

                        // Salva per resume
                        let pending = PendingInterrupt {
                            resume_context: ResumeContext::CommandApproval {
                                command: shell_args.command.clone(),
                            },
                            tool_call_id: Some(tool_call_id),
                            tool_args: Some(args),
                        };
                        state.store_interrupt(&thread_id, pending).await;

                        // Emetti interrupt HITL
                        yield Ok(AgentEvent::updates_with_interrupt(
                            Interrupt::command_approval(
                                &shell_args.command,
                                &shell_args.explanation,
                            )
                        ));
                    }

                    "ask_user" => {
                        // Simile...
                    }

                    _ => {
                        // Tool sconosciuto - errore
                    }
                }
            }
        }
    })
}
```

### 3. state.rs - Aggiungere tool_call_id

```rust
#[derive(Debug, Clone)]
pub struct PendingInterrupt {
    pub resume_context: ResumeContext,
    pub tool_call_id: Option<String>,
    pub tool_args: Option<serde_json::Value>,
}
```

### 4. orchestrator.rs - Resume con tool result

```rust
pub fn create_resume_stream(...) -> EventStream {
    Box::pin(stream! {
        let pending = state.take_interrupt(&thread_id).await?;

        match (&response, &pending.resume_context) {
            (ResumeResponse::Approved, ResumeContext::CommandApproval { command }) => {
                // Esegui comando
                let output = execute_command(command, config.timeout_secs).await;

                // Costruisci tool result per LLM
                let tool_result = ToolResult {
                    tool_call_id: pending.tool_call_id.unwrap(),
                    content: output,
                };

                // Continua conversazione con tool result
                let agent = create_agent_with_tools(&client, &config);
                let history_with_result = /* chat history + tool result */;

                let next_response = agent
                    .completion("", history_with_result)
                    .await?;

                // Gestisci risposta (potrebbe essere altro ToolCall o Message)
                // ...
            }
        }
    })
}
```

---

## Best Practice da Esempi rig-rs

Fonte: https://github.com/0xPlaygrounds/rig/tree/main/rig/rig-core/examples

### 1. Registrazione Tools
```rust
// Metodo 1: singoli tools
.tool(Adder)
.tool(Subtract)

// Metodo 2: vec di tools (per dynamic dispatch)
let tools: Vec<Box<dyn ToolDyn>> = vec![Box::new(Adder), Box::new(Subtract)];
.tools(tools)
```

### 2. multi_turn() per Loop Automatico
```rust
// Esegue automaticamente N round di tool calls
let result = agent
    .prompt("Calculate (3 + 5) / 2")
    .multi_turn(20)  // max 20 rounds
    .await?;
```

**PROBLEMA per HITL**: `multi_turn()` esegue i tools automaticamente!

### 3. Soluzione HITL: multi_turn(1)
```rust
// Ferma dopo il primo tool call
let response = agent
    .prompt(&prompt)
    .multi_turn(1)  // Solo 1 round → si ferma al primo ToolCall
    .await;

// Se response contiene ToolCall, intercettiamo per HITL
// Se response contiene Message, è la risposta finale
```

### 4. Streaming con Tools
```rust
let mut stream = agent.stream_prompt("query").await;
// Tool calls gestiti automaticamente durante streaming
```

---

## Strategia Finale per HITL

```rust
// Usa multi_turn(1) per fermarsi al primo tool call
let response = agent
    .prompt(&prompt)
    .multi_turn(1)
    .await?;

// Il response è una stringa, ma potrebbe contenere tool call metadata
// Dobbiamo usare l'API più bassa per intercettare ModelChoice
```

**Alternativa più pulita**: Usare `completion_request()` invece di `prompt()`:

```rust
use rig::completion::CompletionRequestBuilder;

let response = agent
    .completion_request(&prompt)
    .chat_history(history)
    .send()
    .await?;

// response.choice è ModelChoice::Message o ModelChoice::ToolCall
match response.choice {
    ModelChoice::Message(text) => { /* finale */ }
    ModelChoice::ToolCall { name, id, args } => { /* HITL */ }
}
```

---

## Verifica

1. `cargo build -p infraware-engine --features rig`
2. `cargo test -p infraware-engine --features rig`
3. Test manuale E2E:
   ```bash
   ENGINE_TYPE=rig cargo run -p infraware-backend
   # In altro terminale:
   curl -X POST http://localhost:8080/threads
   # Invia query che richiede comando
   # Verifica interrupt HITL
   # Approva e verifica esecuzione
   ```
