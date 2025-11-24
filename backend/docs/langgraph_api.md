# LangGraph Human-in-the-Loop API Documentation

## Overview

This document explains how the LangGraph API works with human-in-the-loop (HITL) workflows, specifically focusing on command approval for shell execution. This documentation is designed to help developers (particularly those porting to Rust) understand the complete flow, API mechanics, and implementation details.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [API Endpoints](#api-endpoints)
3. [Server-Sent Events (SSE) Stream Format](#server-sent-events-sse-stream-format)
4. [The Interrupt Mechanism](#the-interrupt-mechanism)
5. [Why We Needed to Wrap ShellTool](#why-we-needed-to-wrap-shelltool)
6. [Complete Request/Response Flow](#complete-requestresponse-flow)
7. [Implementation Details](#implementation-details)
8. [Rust Port Considerations](#rust-port-considerations)

---

## Architecture Overview

```
┌─────────────┐         HTTP/SSE          ┌──────────────┐
│   Client    │ ◄────────────────────────► │   LangGraph  │
│  (Python)   │      /threads/.../runs     │   Backend    │
└─────────────┘                            └──────────────┘
                                                   │
                                                   ▼
                                           ┌──────────────┐
                                           │  LangGraph   │
                                           │    Graph     │
                                           └──────────────┘
                                                   │
                                                   ▼
                                           ┌──────────────┐
                                           │ Agent Nodes  │
                                           │  - supervisor│
                                           │  - local_agent│
                                           └──────────────┘
                                                   │
                                                   ▼
                                           ┌──────────────┐
                                           │    Tools     │
                                           │ shell_with_  │
                                           │  approval    │
                                           └──────────────┘
```

The system uses:
- **LangGraph API**: REST API with SSE streaming for real-time updates
- **Agent Graph**: Multi-agent system with supervisor and specialized agents
- **Dynamic Interrupts**: Uses `interrupt()` function to pause execution and request human input
- **Command/Resume Pattern**: Client sends commands to resume interrupted execution

---

## API Endpoints

### Base URL
```
http://127.0.0.1:2024
```

### 1. Create Thread

**Endpoint:** `POST /threads`

**Purpose:** Create a new conversation thread. Threads maintain state across multiple runs.

**Request:**
```json
{
  "metadata": {}
}
```

**Response:**
```json
{
  "thread_id": "a706a38c-983e-405f-89db-3427d6aa9e2f",
  "created_at": "2025-11-20T13:18:53.954852Z",
  "metadata": {}
}
```

**Key Concepts:**
- Each thread represents a stateful conversation
- Thread state persists across runs
- Thread IDs are UUIDs

---

### 2. Stream Run (Primary Endpoint)

**Endpoint:** `POST /threads/{thread_id}/runs/stream`

**Purpose:** Start or resume a run with real-time streaming updates via Server-Sent Events (SSE).

**Initial Request (Start Run):**
```json
{
  "assistant_id": "supervisor",
  "stream_mode": ["values", "updates", "messages"],
  "input": {
    "messages": [
      {
        "role": "user",
        "content": "what os am i running on?"
      }
    ]
  }
}
```

**Resume Request (After Interrupt):**
```json
{
  "assistant_id": "supervisor",
  "stream_mode": ["values", "updates", "messages"],
  "command": {
    "resume": "approved"
  }
}
```

**Parameters:**
- `assistant_id`: The graph/assistant to use (e.g., "supervisor")
- `stream_mode`: Array of event types to stream. Options:
  - `"values"`: Complete state snapshots
  - `"updates"`: Incremental state changes (includes interrupts)
  - `"messages"`: Message updates
- `input`: Initial input to the graph (only on first run)
- `command`: Command to execute (e.g., resume after interrupt)

**Important:** The `"updates"` stream mode is **required** to receive interrupt events.

---

## Server-Sent Events (SSE) Stream Format

SSE is a standard protocol for server-to-client streaming over HTTP. The stream consists of text lines with special prefixes.

### SSE Line Format

```
event: <event_type>
data: <json_payload>
id: <event_id>

```

Each event is terminated by a blank line.

### Example Raw SSE Stream

```
event: metadata
data: {"run_id":"019aa16a-fa02-751e-b1a4-8275c709c6ff","attempt":1}
id: 1763644734855-0

event: values
data: {"messages":[{"content":"what os am i running on?","type":"human","id":"cf33ac54"}]}
id: 1763644735228-0

event: updates
data: {"__interrupt__":[{"value":{"type":"command_approval","command":"uname -a","message":"Do you want to execute this command?\n\nCommand: uname -a\n\nApprove? (Y/n)"},"id":"3d8da70d"}]}
id: 1763644736775-0

: heartbeat

event: end
data: {}
id: 1763644738064-0

```

### Event Types

#### 1. `metadata`
Provides run metadata when execution starts.

```json
{
  "run_id": "019aa16a-fa02-751e-b1a4-8275c709c6ff",
  "attempt": 1
}
```

#### 2. `values`
Complete state snapshot. Contains full graph state including all messages.

```json
{
  "messages": [
    {
      "content": "what os am i running on?",
      "additional_kwargs": {},
      "response_metadata": {},
      "type": "human",
      "name": null,
      "id": "cf33ac54-1c35-4915-83ef-ee1c7a56ab83"
    }
  ]
}
```

#### 3. `updates`
Incremental state updates from graph nodes. **This is where interrupts appear.**

**Regular Update:**
```json
{
  "supervisor": {
    "messages": [
      {
        "content": "I'll help you check the OS",
        "type": "ai",
        "name": "supervisor"
      }
    ]
  }
}
```

**Interrupt Update (Critical!):**
```json
{
  "__interrupt__": [
    {
      "value": {
        "type": "command_approval",
        "command": "uname -a",
        "message": "Do you want to execute this command?\n\nCommand: uname -a\n\nApprove? (Y/n)"
      },
      "id": "3d8da70d427f69f87654fec55ca7a56d"
    }
  ]
}
```

**Key Insight:** Interrupts are **not** a separate event type. They appear as `updates` events with a special `__interrupt__` key in the data payload.

#### 4. `messages`
Contains message-level updates (if stream mode includes "messages").

#### 5. `error`
Indicates an error occurred during execution.

```json
{
  "message": "Error description",
  "type": "error"
}
```

#### 6. `end`
Signals the stream has completed (success or failure).

```json
{}
```

#### 7. Heartbeats
Lines starting with `:` are comments/heartbeats to keep the connection alive:
```
: heartbeat
```

---

## The Interrupt Mechanism

### Dynamic vs Static Interrupts

LangGraph supports two interrupt types:

#### **Static Interrupts** (Not Used Here)
- Configured at graph compile time or runtime
- Parameters: `interrupt_before`, `interrupt_after`
- Pauses execution before/after specific nodes
- Use case: Debugging and testing
- **Limitation:** No mechanism to pass data to the client or receive input back

#### **Dynamic Interrupts** (What We Use)
- Called programmatically with `interrupt()` function
- Pauses execution **within** a node's logic
- Can pass arbitrary data to the client
- Client resumes with `Command(resume=value)`
- The returned value is injected back into the node's execution
- Use case: Production human-in-the-loop workflows

### How Dynamic Interrupts Work

```python
from langgraph.types import interrupt

@tool
def shell_with_approval(commands: str) -> str:
    """Execute shell commands with human approval."""

    # 1. Call interrupt() - execution pauses here
    approval = interrupt(
        {
            "type": "command_approval",
            "command": commands,
            "message": f"Do you want to execute this command?\n\nCommand: {commands}"
        }
    )

    # 2. Execution resumes when client sends Command(resume=...)
    # 3. The resume value is returned by interrupt() and stored in 'approval'

    if approval and str(approval).upper() not in ["N", "NO", "CANCEL"]:
        result = base_shell_tool.invoke({"commands": [commands]})
        return result
    else:
        return "Command execution cancelled by user."
```

**Flow:**
1. Tool calls `interrupt(payload)`
2. LangGraph pauses execution and stores the state
3. Backend sends SSE `updates` event with `__interrupt__` containing the payload
4. Client receives interrupt, displays to user
5. User approves/rejects
6. Client sends new request with `{"command": {"resume": "approved"}}`
7. LangGraph resumes execution, `interrupt()` returns `"approved"`
8. Tool continues with the resume value

---

## Why We Needed to Wrap ShellTool

### The Problem with `ask_human_input=True`

The original implementation used:
```python
from langchain_community.tools import ShellTool

shell_tool = ShellTool(ask_human_input=True)
```

**Why this doesn't work with LangGraph API:**

1. **Uses Python's `input()` function**:
   - `ask_human_input=True` makes ShellTool call Python's built-in `input()`
   - This reads from **stdin** on the **backend server**
   - The backend console (not the client) shows the Y/N prompt
   - Client has no visibility into the approval request

2. **Synchronous blocking**:
   - `input()` blocks the entire backend thread
   - SSE stream stalls and sends only heartbeats
   - No data flows to the client
   - Client doesn't know what's happening

3. **No API integration**:
   - The approval happens in a separate process (terminal I/O)
   - Not part of the LangGraph execution graph
   - Can't be handled through the REST API
   - Breaks the client-server architecture

### The Solution: Custom Wrapper with Dynamic Interrupts

```python
from langchain_community.tools import ShellTool
from langchain_core.tools import tool
from langgraph.types import interrupt

# Create base tool without human input
base_shell_tool = ShellTool(ask_human_input=False)

@tool
def shell_with_approval(commands: str) -> str:
    """Execute shell commands with human approval."""

    # Use LangGraph's interrupt() instead of input()
    approval = interrupt(
        {
            "type": "command_approval",
            "command": commands,
            "message": f"Do you want to execute this command?\n\nCommand: {commands}\n\nApprove? (Y/n)"
        }
    )

    if approval and str(approval).upper() not in ["N", "NO", "CANCEL"]:
        result = base_shell_tool.invoke({"commands": [commands]})
        return result
    else:
        return "Command execution cancelled by user."
```

**Benefits:**

1. **Proper API integration**: Approval request flows through SSE stream
2. **Non-blocking**: Backend doesn't block; state is checkpointed
3. **Stateless server**: Server can restart; state persists in checkpoints
4. **Client control**: Client receives interrupt and controls approval
5. **Structured data**: Payload is structured JSON, not terminal I/O
6. **Resumable**: Can resume from different client or session

---

## Complete Request/Response Flow

### Phase 1: Initial Request

**Client → Server:**
```http
POST /threads/a706a38c-983e-405f-89db-3427d6aa9e2f/runs/stream HTTP/1.1
Content-Type: application/json

{
  "assistant_id": "supervisor",
  "stream_mode": ["values", "updates", "messages"],
  "input": {
    "messages": [
      {"role": "user", "content": "what os am i running on?"}
    ]
  }
}
```

**Server → Client (SSE Stream):**

```
event: metadata
data: {"run_id":"019aa16a-fa02-751e-b1a4-8275c709c6ff","attempt":1}

event: values
data: {"messages":[{"content":"what os am i running on?","type":"human"}]}

event: updates
data: {"supervisor":{"messages":[{"content":"I'll transfer to local agent","type":"ai"}]}}

event: values
data: {"messages":[...transfer completed...]}

event: updates
data: {"__interrupt__":[{"value":{"type":"command_approval","command":"uname -a","message":"Do you want to execute this command?\n\nCommand: uname -a\n\nApprove? (Y/n)"},"id":"3d8da70d"}]}

```

**Stream pauses here** - waiting for client to resume.

### Phase 2: Client Approval

**Client:**
1. Detects `__interrupt__` in updates
2. Extracts command: "uname -a"
3. Shows prompt to user
4. User types "Y"

### Phase 3: Resume Request

**Client → Server:**
```http
POST /threads/a706a38c-983e-405f-89db-3427d6aa9e2f/runs/stream HTTP/1.1
Content-Type: application/json

{
  "assistant_id": "supervisor",
  "stream_mode": ["values", "updates", "messages"],
  "command": {
    "resume": "approved"
  }
}
```

**Server → Client (SSE Stream):**

```
event: metadata
data: {"run_id":"019aa16a-fa02-751e-b1a4-8275c709c6ff","attempt":2}

event: updates
data: {"local_agent":{"messages":[{"content":"Linux hostname 5.15.0...","type":"tool"}]}}

event: values
data: {"messages":[...complete state with results...]}

event: updates
data: {"supervisor":{"messages":[{"content":"You are running on Linux...","type":"ai"}]}}

event: end
data: {}

```

### Phase 4: Client Completion

**Client:**
1. Receives updates with command execution results
2. Receives final AI response
3. Receives `end` event
4. Displays results to user
5. Closes SSE connection

---

## Implementation Details

### Python Client Implementation

#### SSE Parsing

```python
def parse_sse_line(line):
    """Parse a single SSE line."""
    if line.startswith("event: "):
        return ("event", line[7:].strip())
    elif line.startswith("data: "):
        return ("data", line[6:].strip())
    elif line.startswith("id: "):
        return ("id", line[4:].strip())
    return None
```

**Key Points:**
- SSE lines have prefixes: `event:`, `data:`, `id:`
- Fields are separated by `: ` (colon-space)
- Events are terminated by blank lines
- Comments start with `:` (used for heartbeats)

#### Streaming with Requests

```python
response = requests.post(
    url,
    json=payload,
    headers={"Content-Type": "application/json"},
    stream=True  # Critical: enables streaming
)

current_event = None
for line in response.iter_lines():
    if not line:
        continue

    line = line.decode('utf-8')
    parsed = parse_sse_line(line)

    if parsed:
        field, value = parsed
        if field == "event":
            current_event = value
        elif field == "data" and current_event:
            yield (current_event, value)
```

**Key Points:**
- Must use `stream=True` in requests
- Use `iter_lines()` to process line by line
- Maintain `current_event` state across lines
- Yield `(event_type, data)` tuples

#### Detecting Interrupts

```python
for event_type, data in stream_run(thread_id, input_data):
    if event_type == "updates":
        updates = json.loads(data)

        # Check for interrupt
        if "__interrupt__" in updates:
            interrupt_list = updates["__interrupt__"]

            if interrupt_list and len(interrupt_list) > 0:
                interrupt_data = interrupt_list[0]
                value = interrupt_data.get("value", {})

                command = value.get("command")
                message = value.get("message")

                # Show to user and get approval
                print(f"Command: {command}")
                approval = input(message)

                # Break to resume
                break
```

**Key Points:**
- Interrupts appear in `updates` events
- Look for `__interrupt__` key in the JSON
- `__interrupt__` is always an array (can have multiple interrupts)
- Extract the `value` field which contains your custom payload
- Must break the loop to send resume request

#### Resuming Execution

```python
for event_type, data in stream_run(
    thread_id,
    command={"resume": "approved"}  # or user's input
):
    # Handle completion events
    if event_type == "values":
        # Process final state
        pass
    elif event_type == "end":
        # Done!
        break
```

**Key Points:**
- Resume by sending `command` instead of `input`
- The resume value can be anything JSON-serializable
- The value is returned by `interrupt()` in the tool
- Stream format is identical to initial request

---

### Backend Implementation

#### Custom Tool with Interrupt

```python
from langchain_core.tools import tool
from langgraph.types import interrupt
from langchain_community.tools import ShellTool

base_shell_tool = ShellTool(ask_human_input=False)

@tool
def shell_with_approval(commands: str) -> str:
    """
    Execute shell commands with human approval.

    Args:
        commands: Shell command(s) to execute

    Returns:
        Command output or cancellation message
    """
    # Pause and request approval
    approval = interrupt(
        {
            "type": "command_approval",
            "command": commands,
            "message": f"Do you want to execute this command?\n\nCommand: {commands}\n\nApprove? (Y/n)"
        }
    )

    # Check approval
    if approval and str(approval).upper() not in ["N", "NO", "CANCEL"]:
        result = base_shell_tool.invoke({"commands": [commands]})
        return result
    else:
        return "Command execution cancelled by user."
```

**Key Points:**
- Use `@tool` decorator from langchain_core
- Import `interrupt` from `langgraph.types`
- Call `interrupt()` with any JSON-serializable dict
- The value returned by `interrupt()` is what client sends in `resume`
- Continue execution after `interrupt()` returns

#### Agent Configuration

```python
from langchain.agents import create_agent
from agents.shared.models import model

local_agent = create_agent(
    model=model,
    tools=[shell_with_approval],  # Use wrapped tool
    system_prompt=(
        "You are a bash shell assistant agent.\n"
        "Assist with bash-related tasks."
    ),
    name="local_agent",
)
```

**Key Points:**
- Pass custom tool to agent's tools list
- Agent will call tool as normal
- LangGraph handles interrupt automatically
- No special graph configuration needed for dynamic interrupts

---

## Rust Port Considerations

### HTTP Client Requirements

1. **SSE Support**:
   - Need a streaming HTTP client (e.g., `reqwest` with `stream` feature)
   - Must handle chunked transfer encoding
   - Parse line-by-line as data arrives
   - Handle connection keepalive (heartbeats)

2. **Async/Await**:
   - SSE streams are long-lived connections
   - Use async/await for non-blocking I/O
   - Consider `tokio` or `async-std` runtime

3. **Example with reqwest**:
```rust
use reqwest;
use futures::stream::StreamExt;

let client = reqwest::Client::new();
let response = client
    .post(url)
    .json(&payload)
    .send()
    .await?;

let mut stream = response.bytes_stream();
while let Some(chunk) = stream.next().await {
    let bytes = chunk?;
    // Parse SSE lines from bytes
}
```

### SSE Parsing in Rust

```rust
struct SseEvent {
    event_type: Option<String>,
    data: Option<String>,
    id: Option<String>,
}

fn parse_sse_line(line: &str) -> Option<(&str, &str)> {
    if let Some(rest) = line.strip_prefix("event: ") {
        Some(("event", rest.trim()))
    } else if let Some(rest) = line.strip_prefix("data: ") {
        Some(("data", rest.trim()))
    } else if let Some(rest) = line.strip_prefix("id: ") {
        Some(("id", rest.trim()))
    } else {
        None
    }
}

// Usage:
let mut current_event = None;
for line in buffer.lines() {
    if line.is_empty() {
        continue;
    }

    if let Some((field, value)) = parse_sse_line(line) {
        match field {
            "event" => current_event = Some(value.to_string()),
            "data" => {
                if let Some(event_type) = &current_event {
                    // Process (event_type, value)
                }
            }
            _ => {}
        }
    }
}
```

### JSON Handling

Use `serde_json` for JSON parsing:

```rust
use serde_json::Value;

#[derive(Deserialize)]
struct InterruptValue {
    #[serde(rename = "type")]
    interrupt_type: String,
    command: String,
    message: String,
}

#[derive(Deserialize)]
struct Interrupt {
    value: InterruptValue,
    id: String,
}

// Parse updates
let updates: Value = serde_json::from_str(data)?;

if let Some(interrupts) = updates.get("__interrupt__") {
    if let Some(interrupt_array) = interrupts.as_array() {
        if let Some(first_interrupt) = interrupt_array.first() {
            let interrupt: Interrupt = serde_json::from_value(first_interrupt.clone())?;
            println!("Command: {}", interrupt.value.command);
        }
    }
}
```

### State Management

```rust
struct RunContext {
    thread_id: String,
    run_id: Option<String>,
    interrupted: bool,
    interrupt_data: Option<Interrupt>,
}

impl RunContext {
    async fn start_run(&mut self, input: Value) -> Result<()> {
        // Send initial request
        let events = self.stream_run(input, None).await?;

        for (event_type, data) in events {
            match event_type.as_str() {
                "updates" => {
                    if self.check_interrupt(&data)? {
                        self.interrupted = true;
                        break;
                    }
                }
                "end" => break,
                _ => {}
            }
        }

        Ok(())
    }

    async fn resume_run(&mut self, approval: &str) -> Result<()> {
        let command = json!({
            "resume": approval
        });

        let events = self.stream_run(None, Some(command)).await?;
        // Process completion events

        Ok(())
    }
}
```

### Error Handling

```rust
use thiserror::Error;

#[derive(Error, Debug)]
enum LangGraphError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("SSE parse error: {0}")]
    SseParse(String),

    #[error("Run failed: {0}")]
    RunFailed(String),

    #[error("Interrupt not found")]
    InterruptNotFound,
}
```

### Thread Safety

- Use `Arc<Mutex<RunContext>>` if sharing state across threads
- SSE streams are single-threaded (one connection per run)
- Backend handles concurrency; client typically doesn't need it

### Testing

1. **Mock SSE Streams**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn mock_sse_stream() -> Vec<String> {
        vec![
            "event: metadata".to_string(),
            "data: {\"run_id\":\"123\"}".to_string(),
            "".to_string(),
            "event: updates".to_string(),
            "data: {\"__interrupt__\":[{...}]}".to_string(),
            "".to_string(),
        ]
    }

    #[test]
    fn test_parse_interrupt() {
        let stream = mock_sse_stream();
        // Test parsing logic
    }
}
```

2. **Integration Tests**:
   - Spin up test LangGraph server
   - Test full request/resume flow
   - Verify interrupt detection
   - Test error cases

### Performance Considerations

1. **Buffer Management**:
   - SSE data can be large (full state snapshots)
   - Use bounded buffers to prevent memory bloat
   - Consider streaming JSON parsers for large payloads

2. **Connection Pooling**:
   - Reuse HTTP client instances
   - Configure keepalive appropriately
   - Handle connection drops gracefully

3. **Backpressure**:
   - Backend sends data faster than display can render
   - Buffer events or skip intermediate states
   - Only final state usually matters

---

## Key Takeaways for Rust Port

### Must-Have Features

1. ✅ HTTP client with streaming support
2. ✅ SSE parser (line-by-line with prefixes)
3. ✅ JSON serialization/deserialization
4. ✅ Async/await for non-blocking I/O
5. ✅ State machine for run phases (start → interrupt → resume → complete)

### Critical Details

1. **Interrupts are in `updates` events**, not separate event type
2. **Must include `"updates"` in `stream_mode`** array
3. **`__interrupt__` is an array** (even if only one interrupt)
4. **Resume value** can be any JSON value (string, object, etc.)
5. **Empty lines** terminate SSE events
6. **Heartbeats** keep connection alive (`: heartbeat`)

### Architecture Patterns

```
┌─────────────────────────────────────────┐
│           Rust Client                   │
├─────────────────────────────────────────┤
│  1. Thread Manager                      │
│     - Create/list threads               │
│                                         │
│  2. Run Controller                      │
│     - Start runs                        │
│     - Monitor SSE stream                │
│     - Detect interrupts                 │
│     - Resume runs                       │
│                                         │
│  3. SSE Parser                          │
│     - Parse event/data/id lines         │
│     - Buffer incomplete events          │
│     - Yield (event_type, data) tuples   │
│                                         │
│  4. Event Processor                     │
│     - Handle metadata                   │
│     - Process values/updates            │
│     - Extract interrupts                │
│     - Handle errors/end                 │
│                                         │
│  5. UI/Display Layer                    │
│     - Show messages                     │
│     - Prompt for approval               │
│     - Display results                   │
└─────────────────────────────────────────┘
```

### Example High-Level Rust API

```rust
use langgraph_client::Client;

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::new("http://127.0.0.1:2024");

    // Create thread
    let thread = client.create_thread().await?;

    // Start run
    let mut run = thread
        .run("supervisor")
        .with_stream_modes(&["values", "updates", "messages"])
        .with_input(json!({
            "messages": [{"role": "user", "content": "what os am i running on?"}]
        }))
        .start()
        .await?;

    // Process events
    while let Some(event) = run.next().await? {
        match event {
            Event::Message(msg) => println!("AI: {}", msg),
            Event::Interrupt(interrupt) => {
                println!("Command: {}", interrupt.command);
                let approval = prompt_user(&interrupt.message)?;
                run.resume(&approval).await?;
            }
            Event::Complete => break,
            _ => {}
        }
    }

    Ok(())
}
```

---

## Debugging Tips

### Common Issues

1. **No interrupt received**:
   - Check `stream_mode` includes `"updates"`
   - Verify backend is calling `interrupt()`
   - Look for `__interrupt__` in raw SSE data

2. **Stream hangs**:
   - Check for blocking `input()` calls in backend
   - Verify heartbeats are being sent
   - Check network/firewall issues

3. **Resume doesn't work**:
   - Verify sending `command` not `input`
   - Check `thread_id` matches
   - Ensure graph is configured for resumption

4. **JSON parse errors**:
   - Some SSE data can be large (truncate for preview)
   - Handle partial JSON in data field
   - Use streaming JSON parsers for large payloads

### Enable Debug Logging

Python client includes debug output:
```python
print(f"[DEBUG] Raw SSE: {line[:100]}")
print(f"[DEBUG] Event type: {current_event}")
print(f"[DEBUG] Data preview: {value[:100]}")
```

Rust equivalent:
```rust
tracing::debug!("SSE line: {}", &line[..line.len().min(100)]);
tracing::debug!("Event type: {}", event_type);
```

---

## References

- **LangGraph Human-in-the-Loop Docs**: https://docs.langchain.com/langsmith/add-human-in-the-loop
- **SSE Specification**: https://html.spec.whatwg.org/multipage/server-sent-events.html
- **LangGraph API**: https://langchain-ai.github.io/langgraph/cloud/reference/api/
- **Python Client Code**: `langgraph_interactive_run.py`
- **Backend Agent Code**: `../infraware-terminal/backend/src/agents/local/agent.py`

---

## Changelog

- **2025-11-20**: Initial documentation
  - Documented complete API flow
  - Explained interrupt mechanism
  - Added Rust port considerations
  - Included working Python implementation details
