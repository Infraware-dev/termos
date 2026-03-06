# Engine Integration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate the backend binary and HTTP layer, embed the engine directly into the terminal, then collapse the
workspace into a single crate.

**Architecture:** The terminal will own `Arc<dyn AgenticEngine>` directly. `LlmController` drives the engine and
converts `AgentEvent` streams into `AppBackgroundEvent` for the existing UI pipeline. Commands execute via the
terminal's PTY, not internally by the engine. The engine, shared types, and terminal code all live under a single
`Cargo.toml` with no workspace.

**Tech Stack:** Rust 2024, egui/eframe, rig-core 0.28, tokio, futures

**Design doc:** `docs/plans/2026-02-26-engine-integration-design.md`

---

## Phase 1: Integrate Engine Into Terminal

### Task 1: Add engine dependency and create engine module

**Files:**

- Modify: `terminal-app/Cargo.toml`
- Create: `terminal-app/src/engine.rs`

**Step 1: Add `infraware-engine` dependency to terminal**

In `terminal-app/Cargo.toml`, add after the existing dependencies:

```toml
infraware-engine = { path = "../crates/infraware-engine", features = ["rig"] }
```

**Step 2: Create engine wrapper module**

Create `terminal-app/src/engine.rs` that re-exports engine types the terminal needs:

```rust
//! Engine module — re-exports from infraware-engine for terminal use.

pub use infraware_engine::adapters::mock::MockEngine;
#[cfg(feature = "rig")]
pub use infraware_engine::adapters::rig::{RigEngine, RigEngineConfig};
pub use infraware_engine::{
    AgentEvent, AgenticEngine, EngineError, EventStream, HealthStatus, Interrupt, Message,
    MessageRole, ResumeResponse, RunInput, ThreadId,
};
```

**Step 3: Register the module in main.rs**

In `terminal-app/src/main.rs`, add after line 13 (`mod input;`):

```rust
mod engine;
```

**Step 4: Verify it compiles**

Run: `cargo build -p infraware-terminal`
Expected: SUCCESS (engine module is created but not yet used)

**Step 5: Commit**

```
feat: add engine dependency to terminal
```

---

### Task 2: Rewrite LlmController to use engine directly

**Files:**

- Modify: `terminal-app/src/app/llm_controller.rs`
- Modify: `terminal-app/src/app.rs` (AppBackgroundEvent, InfrawareApp fields)

**Step 1: Update AppBackgroundEvent to carry needs_continuation**

In `terminal-app/src/app.rs`, update the `LlmCommandApproval` variant (lines 68-73) to include
`needs_continuation`:

```rust
    LlmCommandApproval {
        /// The command to execute
        command: String,
        /// Message describing why
        message: String,
        /// Whether the agent needs the command output to continue reasoning
        needs_continuation: bool,
    },
```

**Step 2: Rewrite LlmController**

Replace the entire contents of `terminal-app/src/app/llm_controller.rs` with the new engine-based
implementation:

```rust
use std::sync::mpsc;
use std::sync::Arc;

use futures::StreamExt as _;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

use crate::app::AppBackgroundEvent;
use crate::engine::{
    AgentEvent, AgenticEngine, MockEngine, ResumeResponse, RunInput, ThreadId,
};
use crate::llm::IncrementalRenderer;

/// Drives the `AgenticEngine` and converts its event stream into
/// `AppBackgroundEvent` values that the terminal UI can consume.
#[derive(Debug)]
pub struct LlmController {
    engine: Arc<dyn AgenticEngine>,
    thread_id: Option<ThreadId>,
    pub incremental_renderer: IncrementalRenderer,
    bg_event_tx: mpsc::Sender<AppBackgroundEvent>,
    bg_event_rx: mpsc::Receiver<AppBackgroundEvent>,
    cancel_token: Option<CancellationToken>,
}

impl LlmController {
    /// Creates a new controller, selecting the engine from environment.
    pub fn new() -> Self {
        let (bg_event_tx, bg_event_rx) = mpsc::channel();
        let engine = Self::create_engine();

        Self {
            engine,
            thread_id: None,
            incremental_renderer: IncrementalRenderer::new(),
            bg_event_tx,
            bg_event_rx,
            cancel_token: None,
        }
    }

    /// Selects and initialises the engine based on `ENGINE_TYPE` env var.
    fn create_engine() -> Arc<dyn AgenticEngine> {
        let engine_type =
            std::env::var("ENGINE_TYPE").unwrap_or_else(|_| "rig".to_string());

        match engine_type.as_str() {
            #[cfg(feature = "rig")]
            "rig" => match crate::engine::RigEngine::from_env() {
                Ok(engine) => {
                    tracing::info!("Initialised RigEngine (Anthropic Claude)");
                    Arc::new(engine)
                }
                Err(e) => {
                    tracing::warn!("RigEngine init failed ({e}), falling back to MockEngine");
                    Arc::new(MockEngine::new(None))
                }
            },
            _ => {
                tracing::info!("Using MockEngine");
                let workflow = std::env::var("MOCK_WORKFLOW_FILE")
                    .ok()
                    .and_then(|path| {
                        let data = std::fs::read_to_string(&path).ok()?;
                        serde_json::from_str(&data).ok()
                    });
                Arc::new(MockEngine::new(workflow))
            }
        }
    }

    /// Starts a new LLM query, spawning a background task that streams
    /// engine events and forwards them as `AppBackgroundEvent`.
    pub fn start_query(&mut self, runtime: &Runtime, text: String) {
        // Cancel any active query
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }

        self.incremental_renderer.reset();

        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        let engine = Arc::clone(&self.engine);
        let tx = self.bg_event_tx.clone();
        let thread_id = self.thread_id.clone();

        runtime.spawn(async move {
            // Ensure thread exists
            let thread_id = match thread_id {
                Some(id) => id,
                None => match engine.create_thread(None).await {
                    Ok(id) => id,
                    Err(e) => {
                        let _ = tx.send(AppBackgroundEvent::LlmError(format!(
                            "Failed to create thread: {e}"
                        )));
                        return;
                    }
                },
            };

            // Build input and start stream
            let input = RunInput::single_user_message(text);
            let stream = match engine.stream_run(&thread_id, input).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(AppBackgroundEvent::LlmError(format!(
                        "Failed to start run: {e}"
                    )));
                    return;
                }
            };

            Self::consume_event_stream(stream, &tx, &cancel_token).await;
        });
    }

    /// Resumes after user approves a command and the terminal captures
    /// its output from the PTY.
    pub fn resume_with_command_output(
        &mut self,
        runtime: &Runtime,
        command: String,
        output: String,
    ) {
        self.spawn_resume(
            runtime,
            ResumeResponse::command_output(command, output),
        );
    }

    /// Resumes after user answers a question.
    pub fn resume_with_answer(&mut self, runtime: &Runtime, answer: String) {
        self.spawn_resume(runtime, ResumeResponse::answer(answer));
    }

    /// Resumes after user rejects a command.
    pub fn resume_rejected(&mut self, runtime: &Runtime) {
        self.spawn_resume(runtime, ResumeResponse::Rejected);
    }

    /// Cancels the active query.
    pub fn cancel(&mut self) {
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }
    }

    /// Drains all pending background events.
    pub fn poll_events(&self) -> Vec<AppBackgroundEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.bg_event_rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// Stores the thread ID after first successful creation.
    pub fn set_thread_id(&mut self, id: ThreadId) {
        self.thread_id = Some(id);
    }

    // --- private helpers ---

    fn spawn_resume(&mut self, runtime: &Runtime, response: ResumeResponse) {
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }

        self.incremental_renderer.reset();

        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        let engine = Arc::clone(&self.engine);
        let tx = self.bg_event_tx.clone();
        let thread_id = self.thread_id.clone();

        runtime.spawn(async move {
            let Some(thread_id) = thread_id else {
                let _ = tx.send(AppBackgroundEvent::LlmError(
                    "No active thread for resume".to_string(),
                ));
                return;
            };

            let stream = match engine.resume_run(&thread_id, response).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(AppBackgroundEvent::LlmError(format!(
                        "Failed to resume run: {e}"
                    )));
                    return;
                }
            };

            Self::consume_event_stream(stream, &tx, &cancel_token).await;
        });
    }

    /// Consumes an `EventStream` from the engine, converting each
    /// `AgentEvent` into an `AppBackgroundEvent` and sending it over
    /// the channel.
    async fn consume_event_stream(
        mut stream: crate::engine::EventStream,
        tx: &mpsc::Sender<AppBackgroundEvent>,
        cancel_token: &CancellationToken,
    ) {
        use crate::engine::Interrupt;

        while let Some(result) = stream.next().await {
            if cancel_token.is_cancelled() {
                return;
            }

            match result {
                Ok(AgentEvent::Message(msg)) => {
                    let _ = tx.send(AppBackgroundEvent::LlmChunk(msg.content));
                }
                Ok(AgentEvent::Updates { interrupts }) => {
                    if let Some(interrupts) = interrupts {
                        for interrupt in interrupts {
                            let event = match interrupt {
                                Interrupt::CommandApproval {
                                    command,
                                    message,
                                    needs_continuation,
                                } => AppBackgroundEvent::LlmCommandApproval {
                                    command,
                                    message,
                                    needs_continuation,
                                },
                                Interrupt::Question { question, options } => {
                                    AppBackgroundEvent::LlmQuestion { question, options }
                                }
                            };
                            let _ = tx.send(event);
                        }
                    }
                }
                Ok(AgentEvent::Phase { phase }) => {
                    let _ = tx.send(AppBackgroundEvent::LlmPhase(phase));
                }
                Ok(AgentEvent::Error { message }) => {
                    let _ = tx.send(AppBackgroundEvent::LlmError(message));
                }
                Ok(AgentEvent::End) => {
                    let _ = tx.send(AppBackgroundEvent::LlmStreamComplete);
                    return;
                }
                Ok(AgentEvent::Metadata { .. } | AgentEvent::Values { .. }) => {
                    // Metadata and Values are backend-only concerns; skip
                }
                Err(e) => {
                    let _ =
                        tx.send(AppBackgroundEvent::LlmError(format!("Stream error: {e}")));
                    return;
                }
            }
        }

        // Stream ended without explicit End event
        let _ = tx.send(AppBackgroundEvent::LlmStreamComplete);
    }
}
```

**Step 3: Update InfrawareApp to use the new LlmController**

In `terminal-app/src/app.rs`, update the `InfrawareApp` fields and constructor. The `LlmController::new()`
no longer takes `runtime` as a parameter — it reads env vars directly. Update the call site accordingly.

Also update `use` imports: remove `crate::llm::{HttpLLMClient, LLMClientTrait, LLMStreamEvent}` and
`crate::auth::*` imports if present.

**Step 4: Verify it compiles**

Run: `cargo build -p infraware-terminal`
Expected: Compilation errors related to old `LlmResult` variant and auth — we fix those in the next tasks.

**Step 5: Commit**

```
feat: rewrite LlmController to use engine directly
```

---

### Task 3: Update LlmEventHandler and AppMode for new event flow

**Files:**

- Modify: `terminal-app/src/app/llm_event_handler.rs`
- Modify: `terminal-app/src/state.rs`
- Modify: `terminal-app/src/app.rs`

**Step 1: Add `needs_continuation` to `AppMode::AwaitingApproval`**

In `terminal-app/src/state.rs`, update the `AwaitingApproval` variant (line 27) to track
`needs_continuation`:

```rust
    AwaitingApproval {
        command: String,
        message: String,
        needs_continuation: bool,
    },
```

Update the `From<EngineStatus>` impl to pass `needs_continuation` through.

Update `can_transition_to()` — no logic changes needed since it uses `{ .. }` patterns.

**Step 2: Update LlmEventHandler to handle `needs_continuation`**

In `terminal-app/src/app/llm_event_handler.rs`, update the `LlmCommandApproval` handler to store
`needs_continuation`:

```rust
AppBackgroundEvent::LlmCommandApproval {
    command,
    message,
    needs_continuation,
} => {
    self.handle_command_approval(command, message, needs_continuation);
}
```

Update `handle_command_approval()` to accept and store `needs_continuation` in `AppMode`.

**Step 3: Remove the `LlmResult` variant from AppBackgroundEvent**

In `terminal-app/src/app.rs`, remove:

```rust
    LlmResult(crate::llm::LLMQueryResult),
```

and remove the `handle_event` branch for `LlmResult` in `llm_event_handler.rs`.

Also remove the `handle_complete()` method if it only serves `LlmResult`.

**Step 4: Update HITL approval flow in app.rs**

In `terminal-app/src/app.rs`, the `submit_hitl_input` method (around line 710) needs to know
`needs_continuation` when entering `ExecutingCommand` state. Extract it from `AppMode::AwaitingApproval`:

```rust
if let AppMode::AwaitingApproval { ref command, needs_continuation, .. } = session.mode {
    // ... write command to PTY ...
    session.mode = AppMode::ExecutingCommand {
        command: command.clone(),
        needs_continuation,
    };
}
```

Update `AppMode::ExecutingCommand` to include `needs_continuation: bool`.

**Step 5: Update command completion handling**

In `terminal-app/src/app.rs` (around line 338), when command completes, use `needs_continuation` to
decide whether to resume the engine or transition to Normal:

```rust
if command_completed {
    if let AppMode::ExecutingCommand { ref command, needs_continuation } = session.mode {
        if needs_continuation {
            let cmd = command.clone();
            let output = session.output_capture.take_output();
            completed_commands.push((session_id, cmd, output));
            session.mode = AppMode::WaitingLLM;
        } else {
            session.output_capture.take_output(); // discard
            session.mode = AppMode::Normal;
        }
    }
}
```

**Step 6: Verify it compiles**

Run: `cargo build -p infraware-terminal`
Expected: SUCCESS (or minor issues to fix)

**Step 7: Commit**

```
feat: wire needs_continuation through approval flow
```

---

### Task 4: Remove HTTP client, auth module, and reqwest dependency

**Files:**

- Delete: `terminal-app/src/llm/client.rs`
- Delete: `terminal-app/src/auth/authenticator.rs`
- Delete: `terminal-app/src/auth/config.rs`
- Delete: `terminal-app/src/auth/models.rs`
- Delete: `terminal-app/src/auth/mod.rs`
- Modify: `terminal-app/src/main.rs` (remove `mod auth;`)
- Modify: `terminal-app/Cargo.toml` (remove `reqwest`)
- Modify: `terminal-app/src/llm/mod.rs` or wherever client.rs is declared

**Step 1: Delete the auth module**

Remove the entire `terminal-app/src/auth/` directory.

In `terminal-app/src/main.rs`, remove `mod auth;` (line 11).

**Step 2: Delete the HTTP client**

Remove `terminal-app/src/llm/client.rs`.

The llm module should now only contain `renderer.rs` and `incremental_renderer.rs`. Update the module
declaration — if there's no `mod.rs`, the `mod llm;` in `main.rs` may need adjustment. Since the directory
has multiple files, ensure there's a `mod.rs` or the files are properly declared.

**Step 3: Remove reqwest from Cargo.toml**

In `terminal-app/Cargo.toml`, remove:

```toml
reqwest = { workspace = true }
```

**Step 4: Remove infraware-shared from terminal Cargo.toml**

Since the terminal now uses types through `infraware-engine` re-exports (and our `engine.rs` wrapper),
remove:

```toml
infraware-shared = { workspace = true }
```

Update any remaining `use infraware_shared::` imports to go through `crate::engine::` instead.

**Step 5: Clean up all remaining references**

Search for any remaining `use crate::auth::`, `use crate::llm::client`, `HttpLLMClient`,
`MockLLMClient`, `LLMClientTrait`, `LLMStreamEvent`, `AuthConfig`, `HttpAuthenticator` etc. and
remove or replace them.

**Step 6: Verify it compiles and tests pass**

Run: `cargo build -p infraware-terminal && cargo test -p infraware-terminal`
Expected: SUCCESS

**Step 7: Commit**

```
refactor: remove HTTP client, auth module, and reqwest
```

---

### Task 5: Phase 1 verification

**Step 1: Full build**

Run: `cargo build --workspace`
Expected: SUCCESS

**Step 2: Run all tests**

Run: `cargo test --workspace`
Expected: All tests pass

**Step 3: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings

**Step 4: Run fmt**

Run: `cargo +nightly fmt --all`

**Step 5: Commit if any formatting changes**

```
style: format code
```

---

## Phase 2: Flatten to Single Crate

### Task 6: Move terminal-app source to root

**Files:**

- Move: `terminal-app/src/*` → `src/`
- Modify: Root `Cargo.toml`

**Step 1: Move terminal-app source files to root src/**

```bash
# Backup old root src if it exists
mv src src.bak 2>/dev/null || true
# Move terminal source to root
cp -r terminal-app/src src
# Copy terminal CLAUDE.md content will be merged later
```

**Step 2: Rewrite root Cargo.toml as a single crate**

Replace the workspace-based root `Cargo.toml` with a single-crate `Cargo.toml` that:

- Removes `[workspace]` and `[workspace.dependencies]`
- Has a single `[package]` section (use infraware-terminal's metadata)
- Lists all dependencies directly (merge terminal-app + engine + shared deps)
- Adds features: `default = ["rig"]`, `rig = ["dep:chrono", "dep:rig-core", "dep:regex", "dep:schemars"]`
- Keeps the profile settings

Key dependencies to include (merged from all crates):

```toml
[dependencies]
# GUI
egui = "0.33.3"
eframe = { version = "0.33.3", features = ["default", "persistence"] }
egui_tiles = "0.14.1"

# Terminal
portable-pty = "0.9.0"
vte = "0.15"

# Async
tokio = { version = "1.49.0", features = ["rt-multi-thread", "sync", "time", "macros", "io-util", "process", "signal"] }
tokio-util = { version = "0.7.18", features = ["codec"] }
async-trait = "0.1.86"
async-stream = "0.3.6"
futures = "0.3.31"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3.19", features = ["env-filter"] }

# Utilities
dotenvy = "0.15.7"
regex = "1.11"
uuid = { version = "1.12", features = ["v4", "serde"] }
image = { version = "0.25", default-features = false, features = ["png"] }
which = "8.0"
ctrlc = "3.4"
once_cell = "1.20"
smallvec = "1.13"
mimalloc = "0.1"
bitflags = "2.9"
arboard = "3.4"
syntect = { version = "5.3.0", features = ["default-fancy"] }

# AI / LLM (optional, behind rig feature)
rig-core = { version = "0.28", optional = true }
chrono = { version = "0.4", optional = true, features = ["serde"] }
schemars = { version = "0.8", optional = true }

[dev-dependencies]
tempfile = "3"

[features]
default = ["rig"]
rig = ["dep:chrono", "dep:rig-core", "dep:schemars"]
```

**Step 3: Verify basic structure**

Run: `ls src/main.rs src/app.rs src/engine.rs`
Expected: All files exist

**Step 4: Attempt build (expect import errors)**

Run: `cargo build`
Expected: FAIL with import errors — we fix those in the next task

**Step 5: Commit**

```
refactor: move terminal source to root crate
```

---

### Task 7: Move engine code into src/engine/

**Files:**

- Move: `crates/infraware-engine/src/*` → `src/engine/`
- Move: `crates/shared/src/*` → `src/engine/shared/`
- Modify: `src/engine.rs` → becomes `src/engine/mod.rs`

**Step 1: Move engine source**

```bash
# Remove the thin wrapper
rm src/engine.rs
# Create engine module directory
mkdir -p src/engine
# Copy engine source files
cp crates/infraware-engine/src/traits.rs src/engine/
cp crates/infraware-engine/src/types.rs src/engine/
cp crates/infraware-engine/src/error.rs src/engine/
cp -r crates/infraware-engine/src/adapters src/engine/
```

**Step 2: Move shared types into engine/shared/**

```bash
mkdir -p src/engine/shared
cp crates/shared/src/models.rs src/engine/shared/
cp crates/shared/src/events.rs src/engine/shared/
cp crates/shared/src/status.rs src/engine/shared/
```

**Step 3: Create src/engine/shared/mod.rs**

```rust
pub mod events;
pub mod models;
pub mod status;

pub use events::{AgentEvent, IncidentPhase, Interrupt, MessageEvent};
pub use models::{
    LLMQueryResult, MAX_THREAD_ID_LENGTH, Message, MessageRole, RunInput, ThreadId, ThreadIdError,
};
pub use status::EngineStatus;
```

**Step 4: Create src/engine/mod.rs**

```rust
pub mod adapters;
mod error;
pub mod shared;
mod traits;
mod types;

pub use error::EngineError;
pub use shared::{
    AgentEvent, IncidentPhase, Interrupt, LLMQueryResult, Message, MessageEvent, MessageRole,
    RunInput, ThreadId,
};
pub use traits::{AgenticEngine, EventStream};
pub use types::{HealthStatus, ResumeResponse};
```

**Step 5: Update all imports in engine files**

In every file under `src/engine/`:

- Replace `use infraware_shared::` with `use crate::engine::shared::`
- Replace `use crate::` (engine-internal) paths as needed since the module hierarchy changed

In the adapters, the rig module files reference `infraware_shared` — update all to
`crate::engine::shared`.

**Step 6: Update all imports in terminal files**

In every file under `src/` (terminal code):

- Replace `use infraware_shared::` with `use crate::engine::shared::`
- Replace `use infraware_engine::` with `use crate::engine::`
- The `crate::engine::` wrapper already re-exports the needed types

**Step 7: Attempt build**

Run: `cargo build`
Expected: May have remaining import issues — fix iteratively

**Step 8: Commit**

```
refactor: move engine and shared code into src/engine/
```

---

### Task 8: Fix all remaining import errors and get green build

**Files:**

- Modify: Various files with broken imports

**Step 1: Fix compilation errors iteratively**

Run `cargo build` and fix each error. Common patterns:

- `use infraware_shared::X` → `use crate::engine::shared::X` or `use crate::engine::X`
- `use infraware_engine::X` → `use crate::engine::X`
- Module path changes in engine internals (e.g., `crate::adapters::` → `crate::engine::adapters::`)

**Step 2: Verify full build**

Run: `cargo build`
Expected: SUCCESS

**Step 3: Run tests**

Run: `cargo test`
Expected: All tests pass

**Step 4: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

**Step 5: Run fmt**

Run: `cargo +nightly fmt --all`

**Step 6: Commit**

```
refactor: fix all imports for single-crate layout
```

---

## Phase 3: Clean Up

### Task 9: Delete old crate directories

**Files:**

- Delete: `crates/infraware-backend/`
- Delete: `crates/shared/`
- Delete: `crates/infraware-engine/`
- Delete: `terminal-app/`

**Step 1: Remove old directories**

```bash
rm -rf crates/
rm -rf terminal-app/
```

**Step 2: Verify build still works**

Run: `cargo build && cargo test`
Expected: SUCCESS (all code is now in src/)

**Step 3: Commit**

```
chore: remove old workspace crate directories
```

---

### Task 10: Remove engine-internal command execution

**Files:**

- Modify: `src/engine/adapters/rig/orchestrator.rs`

**Step 1: Remove command execution functions**

In `src/engine/adapters/rig/orchestrator.rs`, remove these functions:

- `execute_command()` (lines ~503-516)
- `execute_command_with_sudo_password()` (lines ~521-614)
- `validate_command()` (lines ~428-469)
- `check_sudo_password_required()` (lines ~472-496)

**Step 2: Simplify the `ResumeResponse::Approved` path**

In the `create_resume_stream()` match (lines ~639-700), the `Approved` + `CommandApproval` arm should
now simply emit a message saying the command was approved and end — since the terminal handles execution:

```rust
(ResumeResponse::Approved, ResumeContext::CommandApproval { command, needs_continuation }) => {
    // Terminal handles actual execution via PTY.
    // This path should not normally be reached in the new architecture,
    // but we keep it as a safe fallback.
    yield AgentEvent::Message(MessageEvent::assistant(
        format!("Command `{}` approved for execution.", command),
    ));
    yield AgentEvent::end();
}
```

Or remove the `Approved` variant from `ResumeResponse` entirely if the terminal never sends it.

**Step 3: Remove shell module if unused**

Check if `src/engine/adapters/rig/shell.rs` is only used by the removed functions. If so, remove it.

**Step 4: Verify build and tests**

Run: `cargo build && cargo test`
Expected: SUCCESS

**Step 5: Commit**

```
refactor: remove engine-internal command execution
```

---

### Task 11: Clean up legacy types and unused code

**Files:**

- Modify: `src/engine/shared/models.rs` (remove `LLMQueryResult`)
- Modify: `src/engine/shared/mod.rs` (remove `LLMQueryResult` export)
- Modify: `src/engine/mod.rs` (remove `LLMQueryResult` re-export)

**Step 1: Remove LLMQueryResult**

This type is legacy and unused. Remove it from `models.rs`, its export from `shared/mod.rs`, and its
re-export from `engine/mod.rs`.

**Step 2: Search for any remaining references**

```bash
cargo build 2>&1 | head -50
```

Fix any remaining compilation errors.

**Step 3: Verify**

Run: `cargo build && cargo test`
Expected: SUCCESS

**Step 4: Commit**

```
refactor: remove legacy LLMQueryResult type
```

---

### Task 12: Update documentation

**Files:**

- Modify: `CLAUDE.md`
- Delete: `terminal-app/CLAUDE.md` (already removed with terminal-app/)

**Step 1: Update CLAUDE.md**

Update the root `CLAUDE.md` to reflect the new single-crate structure:

- Remove workspace structure section, replace with single-crate layout
- Remove backend crate documentation
- Remove shared crate documentation
- Update build commands (no more `-p` flags needed)
- Update module structure to include `engine/` module
- Remove `INFRAWARE_BACKEND_URL` env var
- Remove auth-related env vars
- Update architecture diagram (no HTTP layer)
- Update CI pipeline section if applicable

**Step 2: Verify no stale references**

Search CLAUDE.md for "backend", "shared", "workspace", "reqwest", "HTTP" and ensure all references are
updated or removed.

**Step 3: Commit**

```
docs: update CLAUDE.md for single-crate architecture
```

---

### Task 13: Final verification

**Step 1: Full build**

Run: `cargo build`
Expected: SUCCESS

**Step 2: All tests**

Run: `cargo test`
Expected: All tests pass

**Step 3: Clippy strict**

Run: `cargo clippy -- -D warnings`
Expected: No warnings

**Step 4: Format check**

Run: `cargo +nightly fmt --all --check`
Expected: No formatting issues

**Step 5: Verify the binary runs**

Run: `ENGINE_TYPE=mock cargo run`
Expected: Terminal launches with MockEngine (no API key needed)

**Step 6: Commit if any final fixes**

```
chore: final cleanup after single-crate migration
```

---

## Summary

| Phase | Tasks | What happens |
|-------|-------|-------------|
| Phase 1 | Tasks 1-5 | Engine integrated into terminal, HTTP client removed |
| Phase 2 | Tasks 6-8 | Workspace flattened into single crate |
| Phase 3 | Tasks 9-13 | Old directories deleted, dead code removed, docs updated |

Total: 13 tasks, each independently committable and verifiable.
