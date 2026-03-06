# Arena Mode Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add Arena mode — a Docker-container-backed PTY session for incident investigation challenges.

**Architecture:** New `PtyProvider::ArenaScenario` variant backed by `ArenaPtySession`, a fork of `TestContainerPtySession` that pulls a user-specified Docker image. On startup it reads `/arena/scenario.json` from inside the container and prints the incident prompt. The SIM_FIXTURE simulation system is removed entirely.

**Tech Stack:** Rust, bollard (Docker API), serde, clap, tokio

---

### Task 1: Remove SIM_FIXTURE simulation code

**Files:**
- Modify: `src/agent/adapters/rig/shell.rs`

**Step 1: Delete simulation types and functions**

Remove the following from `src/agent/adapters/rig/shell.rs`:
- `SimFixture` struct (lines 15-26)
- `SimEntry` struct (lines 28-33)
- `default_fallback()` function (lines 35-37)
- `simulate_command()` function (lines 45-89)
- The `SIM_FIXTURE` env var check in `spawn_command()` (lines 93-95)
- `use regex::Regex;` import (line 4)

The `spawn_command()` function should become:

```rust
pub(super) async fn spawn_command(command: &str, timeout_secs: u64) -> String {
    let effective_timeout = timeout_secs.min(60);

    let child = match Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return format!("Failed to spawn command: {}", e),
    };

    match timeout(
        Duration::from_secs(effective_timeout),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.success() {
                if stdout.trim().is_empty() && stderr.trim().is_empty() {
                    "(Command executed successfully, no output)".to_string()
                } else {
                    format!("{}{}", stdout, stderr)
                }
            } else {
                format!(
                    "Exit code: {}\n{}{}",
                    output.status.code().unwrap_or(-1),
                    stdout,
                    stderr
                )
            }
        }
        Ok(Err(e)) => format!("Failed to execute command: {}", e),
        Err(_) => format!("Command timed out after {} seconds", effective_timeout),
    }
}
```

**Step 2: Delete all sim-related tests**

Remove these test functions from the `mod tests` block:
- `simulate_exact_pattern_match`
- `simulate_regex_wildcard_match`
- `simulate_fallback_on_no_match`
- `simulate_default_fallback_when_field_absent`
- `simulate_invalid_fixture_path_returns_error_string`
- `simulate_invalid_json_returns_error_string`
- `simulate_fixture_allows_description_field`

Also remove test helpers that are no longer used:
- `use std::io::Write as _;`
- `use tempfile::NamedTempFile;`
- `fn write_fixture()`

If the `mod tests` block is now empty, remove it entirely.

**Step 3: Remove unused dependencies**

Check if `regex` is still used elsewhere in the crate. If `shell.rs` was the only consumer, remove `regex = "1"` from `Cargo.toml`. Same for `tempfile` in `[dev-dependencies]` if only used by these tests.

**Step 4: Verify it compiles**

Run: `cargo build --all-features`
Expected: Compiles successfully with no errors.

**Step 5: Run tests**

Run: `cargo test`
Expected: All remaining tests pass.

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor(sim): remove SIM_FIXTURE simulation system"
```

---

### Task 2: Add `arena` feature flag and CLI argument

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/args.rs`
- Modify: `src/main.rs`
- Modify: `src/pty/manager.rs`

**Step 1: Add `arena` feature to Cargo.toml**

In `Cargo.toml`, add to the `[features]` section:

```toml
arena = ["dep:bollard"]
```

**Step 2: Add CLI argument**

In `src/args.rs`, add to the `Args` struct after the `use_pty_test_container` field:

```rust
/// Start in Arena mode with the given Docker image for incident investigation challenges.
#[cfg(feature = "arena")]
#[arg(long)]
pub arena: Option<String>,
```

**Step 3: Add `PtyProvider::ArenaScenario` variant**

In `src/pty/manager.rs`, add to the `PtyProvider` enum:

```rust
#[cfg(feature = "arena")]
ArenaScenario(String),
```

**Step 4: Wire CLI flag to PtyProvider in main.rs**

In `src/main.rs`, update the `app_options()` function. The arena flag should take priority over test_container:

```rust
fn app_options(args: &Args) -> AppOptions {
    #[cfg(feature = "arena")]
    if let Some(ref image) = args.arena {
        return AppOptions {
            pty_provider: pty::PtyProvider::ArenaScenario(image.clone()),
        };
    }

    #[cfg(feature = "pty-test_container")]
    let pty_provider = if args.use_pty_test_container {
        pty::PtyProvider::TestContainer
    } else {
        pty::PtyProvider::Local
    };
    #[cfg(not(feature = "pty-test_container"))]
    let pty_provider = pty::PtyProvider::Local;

    AppOptions { pty_provider }
}
```

**Step 5: Add placeholder match arm in PtyManager::new()**

In `src/pty/manager.rs`, add a temporary match arm in `PtyManager::new()` so the code compiles:

```rust
#[cfg(feature = "arena")]
PtyProvider::ArenaScenario(_image) => {
    todo!("ArenaPtySession not yet implemented")
}
```

**Step 6: Verify it compiles**

Run: `cargo build --features arena`
Expected: Compiles successfully.

Run: `cargo build` (default features, no arena)
Expected: Compiles successfully — arena code is gated.

**Step 7: Commit**

```bash
git add -A
git commit -m "feat(arena): add arena feature flag and CLI argument"
```

---

### Task 3: Create ScenarioManifest struct

**Files:**
- Create: `src/pty/adapters/arena/scenario.rs`

**Step 1: Write the test**

At the bottom of `scenario.rs`, add tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_manifest() {
        let json = r#"{
            "title": "The Cascade",
            "prompt": {
                "title": "INCIDENT ALERT - Priority: High",
                "body": "Checkout Success Rate dropped below 85%",
                "environment": "Kubernetes cluster prod-eu-west-1",
                "mission": "Identify the root cause"
            }
        }"#;
        let manifest: ScenarioManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.title, "The Cascade");
        assert_eq!(manifest.prompt.title, "INCIDENT ALERT - Priority: High");
        assert_eq!(
            manifest.prompt.body,
            "Checkout Success Rate dropped below 85%"
        );
        assert_eq!(
            manifest.prompt.environment.as_deref(),
            Some("Kubernetes cluster prod-eu-west-1")
        );
        assert_eq!(
            manifest.prompt.mission.as_deref(),
            Some("Identify the root cause")
        );
    }

    #[test]
    fn deserialize_minimal_manifest() {
        let json = r#"{
            "title": "Minimal",
            "prompt": {
                "title": "Alert",
                "body": "Something broke"
            }
        }"#;
        let manifest: ScenarioManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.title, "Minimal");
        assert!(manifest.prompt.environment.is_none());
        assert!(manifest.prompt.mission.is_none());
    }

    #[test]
    fn format_prompt_full() {
        let manifest = ScenarioManifest {
            title: "The Cascade".to_string(),
            prompt: ScenarioPrompt {
                title: "INCIDENT ALERT".to_string(),
                body: "Checkout rate dropped".to_string(),
                environment: Some("prod-eu-west-1".to_string()),
                mission: Some("Find root cause".to_string()),
            },
        };
        let output = manifest.format_prompt();
        assert!(output.contains("INCIDENT ALERT"));
        assert!(output.contains("Checkout rate dropped"));
        assert!(output.contains("prod-eu-west-1"));
        assert!(output.contains("Find root cause"));
    }

    #[test]
    fn format_prompt_minimal() {
        let manifest = ScenarioManifest {
            title: "Test".to_string(),
            prompt: ScenarioPrompt {
                title: "Alert".to_string(),
                body: "Broke".to_string(),
                environment: None,
                mission: None,
            },
        };
        let output = manifest.format_prompt();
        assert!(output.contains("Alert"));
        assert!(output.contains("Broke"));
        assert!(!output.contains("Environment"));
        assert!(!output.contains("Mission"));
    }
}
```

**Step 2: Write the implementation**

Create `src/pty/adapters/arena/scenario.rs`:

```rust
//! Scenario manifest read from `/arena/scenario.json` inside the container.

use serde::Deserialize;

/// Top-level scenario manifest.
#[derive(Debug, Deserialize)]
pub struct ScenarioManifest {
    pub title: String,
    pub prompt: ScenarioPrompt,
}

/// Incident prompt displayed to the user at the start of a challenge.
#[derive(Debug, Deserialize)]
pub struct ScenarioPrompt {
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub mission: Option<String>,
}

impl ScenarioManifest {
    /// Format the scenario prompt for terminal display.
    ///
    /// Returns a string with ANSI escape codes for styling.
    pub fn format_prompt(&self) -> String {
        let mut out = String::new();
        out.push_str("\r\n");
        out.push_str(&format!("\x1b[1;31m{}\x1b[0m\r\n", self.prompt.title));
        out.push_str(&format!("\r\n{}\r\n", self.prompt.body));
        if let Some(ref env) = self.prompt.environment {
            out.push_str(&format!("\r\n\x1b[1mEnvironment:\x1b[0m {env}\r\n"));
        }
        if let Some(ref mission) = self.prompt.mission {
            out.push_str(&format!("\r\n\x1b[1mMission:\x1b[0m {mission}\r\n"));
        }
        out.push_str("\r\n");
        out
    }
}
```

**Step 3: Create the arena module files**

Create `src/pty/adapters/arena/mod.rs` (minimal, just declares scenario for now):

```rust
//! Arena PTY session adapter for incident investigation challenges.
//!
//! Runs a user-specified Docker image and presents the scenario prompt.

mod scenario;

pub use self::scenario::ScenarioManifest;
```

**Step 4: Wire the module**

In `src/pty/adapters.rs`, add:

```rust
#[cfg(feature = "arena")]
mod arena;
```

(No pub use yet — we'll add it in the next task when `ArenaPtySession` exists.)

**Step 5: Run tests**

Run: `cargo test --features arena scenario`
Expected: All 4 scenario tests pass.

**Step 6: Commit**

```bash
git add -A
git commit -m "feat(arena): add ScenarioManifest struct"
```

---

### Task 4: Create ArenaContainer

**Files:**
- Create: `src/pty/adapters/arena/container.rs`

**Step 1: Write ArenaContainer**

Fork `src/pty/adapters/test_container/container.rs` into `src/pty/adapters/arena/container.rs`. Changes from the original:

- Rename `Container` to `ArenaContainer`
- `setup()` takes `image: &str` parameter instead of hardcoding `debian:bookworm-slim`
- `pull_image()` parses `image` into repo and tag (split on `:`, default tag `latest`)
- `create_container()` uses the provided image
- Add `exec_read_file()` method to read `/arena/scenario.json` from inside the container

```rust
//! Docker container for arena scenario sessions.

use std::pin::Pin;

use bollard::Docker;
use bollard::config::ContainerCreateBody;
use bollard::container::LogOutput;
use bollard::query_parameters::{
    AttachContainerOptionsBuilder, CreateContainerOptionsBuilder, CreateExecOptionsBuilder,
    CreateImageOptionsBuilder, RemoveContainerOptionsBuilder, ResizeContainerTTYOptionsBuilder,
    StartExecOptionsBuilder,
};
use futures::{Stream, StreamExt};
use tokio::io::AsyncWrite;

/// Container IO streams for interacting with the container's stdin, stdout, and stderr.
pub struct IoHandles {
    pub input: Pin<Box<dyn AsyncWrite + Send>>,
    pub output: Pin<Box<dyn Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send>>,
}

/// Docker container for an arena scenario session.
pub struct ArenaContainer {
    docker: Docker,
    name: String,
    image: String,
}

impl ArenaContainer {
    /// Sets up a container from the given image and returns a handle with IO streams.
    pub async fn setup(image: &str) -> anyhow::Result<(ArenaContainer, IoHandles)> {
        let docker = Docker::connect_with_local_defaults()?;
        let name = format!("infraware_arena_{}", uuid::Uuid::new_v4());
        let container = ArenaContainer {
            docker,
            name,
            image: image.to_string(),
        };
        container.pull_image().await?;
        container.create_container().await?;
        container.start_container().await?;
        let container_io = container.attach_container().await?;

        Ok((container, container_io))
    }

    /// Read a file from inside the running container via `docker exec cat <path>`.
    pub async fn exec_read_file(&self, path: &str) -> anyhow::Result<String> {
        let exec = self
            .docker
            .create_exec(
                &self.name,
                CreateExecOptionsBuilder::default()
                    .attach_stdout(true)
                    .attach_stderr(true)
                    .cmd(vec!["cat".into(), path.into()].into())
                    .build(),
            )
            .await?;

        let start_opts = StartExecOptionsBuilder::default().build();
        let mut output = self.docker.start_exec(&exec.id, Some(start_opts));
        let mut result = String::new();
        while let Some(chunk) = output.next().await {
            match chunk? {
                bollard::container::LogOutput::StdOut { message } => {
                    result.push_str(&String::from_utf8_lossy(&message));
                }
                bollard::container::LogOutput::StdErr { message } => {
                    let stderr = String::from_utf8_lossy(&message);
                    if !stderr.trim().is_empty() {
                        return Err(anyhow::anyhow!(
                            "Error reading {path} from container: {stderr}"
                        ));
                    }
                }
                _ => {}
            }
        }

        Ok(result)
    }

    /// Resizes the container's TTY to the given dimensions.
    pub async fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        let opts = ResizeContainerTTYOptionsBuilder::default()
            .w(i32::from(cols))
            .h(i32::from(rows))
            .build();
        self.docker.resize_container_tty(&self.name, opts).await?;
        Ok(())
    }

    /// Returns `true` if the container is currently running.
    pub async fn is_running(&self) -> bool {
        self.docker
            .inspect_container(&self.name, None)
            .await
            .ok()
            .and_then(|info| info.state)
            .and_then(|state| state.running)
            .unwrap_or(false)
    }

    /// Stops and removes the container.
    pub async fn stop(&self) -> anyhow::Result<()> {
        tracing::debug!("Stopping arena container {}", self.name);
        if let Err(e) = self.docker.stop_container(&self.name, None).await {
            tracing::debug!(
                "Stop request for arena container {} returned error (will still attempt removal): {e}",
                self.name
            );
        } else {
            tracing::debug!("Stopped arena container {}", self.name);
        }

        let opts = RemoveContainerOptionsBuilder::default()
            .force(true)
            .build();
        self.docker.remove_container(&self.name, Some(opts)).await?;
        tracing::debug!("Removed arena container {}", self.name);

        Ok(())
    }

    async fn pull_image(&self) -> anyhow::Result<()> {
        let (repo, tag) = parse_image_ref(&self.image);
        let options = CreateImageOptionsBuilder::default()
            .from_image(repo)
            .tag(tag)
            .build();
        tracing::debug!("Pulling arena image: {options:?}");
        let mut pull_stream = self.docker.create_image(Some(options), None, None);

        let mut image_info = None;
        while let Some(token) = pull_stream.next().await {
            let info = token?;
            image_info = Some(info);
            tracing::debug!("Pulling image... progress: {image_info:?}");
        }
        let Some(image_info) = image_info else {
            return Err(anyhow::anyhow!(
                "Failed to pull image: no information received"
            ));
        };
        tracing::debug!("Arena image pulled; image info: {image_info:?}");

        Ok(())
    }

    async fn create_container(&self) -> anyhow::Result<()> {
        tracing::debug!("Creating arena container: {}", self.name);

        let options = CreateContainerOptionsBuilder::default()
            .name(&self.name)
            .build();

        let config = ContainerCreateBody {
            image: Some(self.image.clone()),
            tty: Some(true),
            open_stdin: Some(true),
            ..Default::default()
        };

        self.docker.create_container(Some(options), config).await?;
        tracing::debug!("Created arena container: {}", self.name);
        Ok(())
    }

    async fn start_container(&self) -> anyhow::Result<()> {
        tracing::debug!("Starting arena container: {}", self.name);
        self.docker.start_container(&self.name, None).await?;
        tracing::debug!("Started arena container: {}", self.name);

        Ok(())
    }

    async fn attach_container(&self) -> anyhow::Result<IoHandles> {
        let options = AttachContainerOptionsBuilder::default()
            .stderr(true)
            .stdout(true)
            .stdin(true)
            .stream(true)
            .build();
        tracing::debug!("Attaching arena container: {options:?}");
        let attach = self
            .docker
            .attach_container(&self.name, Some(options))
            .await?;
        tracing::debug!("Attached arena container: {}", self.name);

        Ok(IoHandles {
            input: attach.input,
            output: attach.output,
        })
    }
}

/// Parse a Docker image reference into (repo, tag).
/// "nginx:1.25" -> ("nginx", "1.25")
/// "nginx" -> ("nginx", "latest")
/// "registry.io/org/image:v2" -> ("registry.io/org/image", "v2")
fn parse_image_ref(image: &str) -> (&str, &str) {
    match image.rsplit_once(':') {
        Some((repo, tag)) if !repo.is_empty() && !tag.contains('/') => (repo, tag),
        _ => (image, "latest"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_image_with_tag() {
        assert_eq!(parse_image_ref("nginx:1.25"), ("nginx", "1.25"));
    }

    #[test]
    fn parse_image_without_tag() {
        assert_eq!(parse_image_ref("nginx"), ("nginx", "latest"));
    }

    #[test]
    fn parse_image_with_registry_and_tag() {
        assert_eq!(
            parse_image_ref("registry.io/org/image:v2"),
            ("registry.io/org/image", "v2")
        );
    }

    #[test]
    fn parse_image_with_registry_no_tag() {
        assert_eq!(
            parse_image_ref("registry.io/org/image"),
            ("registry.io/org/image", "latest")
        );
    }

    #[test]
    fn parse_image_with_port_in_registry() {
        assert_eq!(
            parse_image_ref("localhost:5000/myimage:v1"),
            ("localhost:5000/myimage", "v1")
        );
    }
}
```

**Step 2: Declare container module in arena/mod.rs**

Update `src/pty/adapters/arena/mod.rs`:

```rust
//! Arena PTY session adapter for incident investigation challenges.
//!
//! Runs a user-specified Docker image and presents the scenario prompt.

pub(super) mod container;
mod scenario;

pub use self::scenario::ScenarioManifest;
```

**Step 3: Run tests**

Run: `cargo test --features arena parse_image`
Expected: All 5 parse_image tests pass.

**Step 4: Commit**

```bash
git add -A
git commit -m "feat(arena): add ArenaContainer with configurable image"
```

---

### Task 5: Create ArenaPtySession

**Files:**
- Create: `src/pty/adapters/arena/mod.rs` (update)
- Modify: `src/pty/adapters.rs`
- Modify: `src/pty/manager.rs`

**Step 1: Write ArenaPtySession in arena/mod.rs**

Replace `src/pty/adapters/arena/mod.rs` with the full adapter. This is a fork of `TestContainerPtySession` with the additions for arena (configurable image, scenario manifest reading, prompt injection):

```rust
//! Arena PTY session adapter for incident investigation challenges.
//!
//! Runs a user-specified Docker image and presents the scenario prompt
//! read from `/arena/scenario.json` inside the container.

pub(super) mod container;
mod scenario;

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;

use anyhow::{Context, Result};
use async_trait::async_trait;
use bollard::container::LogOutput;
use futures::{Stream, StreamExt as _};
use portable_pty::PtySize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use self::container::ArenaContainer;
pub use self::scenario::ScenarioManifest;
use crate::pty::io::{PtyReader, PtyWriter};
use crate::pty::traits::PtySession;

/// Path to the scenario manifest inside the container.
const SCENARIO_MANIFEST_PATH: &str = "/arena/scenario.json";

/// Boxed async output stream from the container's TTY.
type OutputStream =
    Pin<Box<dyn Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send>>;

/// PTY session adapter for Arena mode.
///
/// Pulls the specified Docker image, starts a container, reads the scenario
/// manifest from `/arena/scenario.json`, and injects the incident prompt
/// into the terminal output before handing off to the live PTY stream.
pub struct ArenaPtySession {
    container: Option<ArenaContainer>,
    runtime_handle: tokio::runtime::Handle,
    output: std::sync::Mutex<Option<OutputStream>>,
    writer_handle: Option<std::os::unix::net::UnixStream>,
    sigint_handle: Arc<std::sync::Mutex<std::os::unix::net::UnixStream>>,
    /// Scenario prompt text to inject before the live PTY stream.
    scenario_prompt: Option<String>,
}

impl Drop for ArenaPtySession {
    fn drop(&mut self) {
        let Some(container) = self.container.take() else {
            return;
        };
        let handle = self.runtime_handle.clone();
        let join_handle = std::thread::spawn(move || {
            if let Err(e) = handle.block_on(container.stop()) {
                tracing::error!("Failed to stop arena container on drop: {e}");
            }
        });
        if let Err(e) = join_handle.join() {
            tracing::error!("Arena container cleanup thread panicked: {e:?}");
        }
    }
}

impl std::fmt::Debug for ArenaPtySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let has_output = self
            .output
            .lock()
            .map(|o| o.is_some())
            .unwrap_or(false);
        f.debug_struct("ArenaPtySession")
            .field("has_container", &self.container.is_some())
            .field("has_output", &has_output)
            .field("has_writer", &self.writer_handle.is_some())
            .finish_non_exhaustive()
    }
}

impl ArenaPtySession {
    /// Creates a new Arena PTY session from the given Docker image.
    ///
    /// Pulls the image, starts the container, reads the scenario manifest,
    /// and prepares the prompt for injection on first read.
    pub async fn new(image: &str) -> Result<Self> {
        let (container, handles) = ArenaContainer::setup(image).await?;

        // Read scenario manifest from inside the container
        let scenario_prompt = match container
            .exec_read_file(SCENARIO_MANIFEST_PATH)
            .await
        {
            Ok(json) => match serde_json::from_str::<ScenarioManifest>(&json) {
                Ok(manifest) => {
                    tracing::info!(
                        "Arena scenario loaded: {}",
                        manifest.title
                    );
                    Some(manifest.format_prompt())
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse {SCENARIO_MANIFEST_PATH}: {e}"
                    );
                    None
                }
            },
            Err(e) => {
                tracing::warn!(
                    "No scenario manifest found at {SCENARIO_MANIFEST_PATH}: {e}"
                );
                None
            }
        };

        let (unix_read, unix_write) = std::os::unix::net::UnixStream::pair()
            .context("Failed to create Unix socket pair for writer bridge")?;

        let sigint_writer = unix_write
            .try_clone()
            .context("Failed to clone Unix socket for sigint")?;

        spawn_writer_bridge(unix_read, handles.input)?;

        let runtime_handle = tokio::runtime::Handle::current();

        Ok(Self {
            container: Some(container),
            runtime_handle,
            output: std::sync::Mutex::new(Some(handles.output)),
            writer_handle: Some(unix_write),
            sigint_handle: Arc::new(std::sync::Mutex::new(sigint_writer)),
            scenario_prompt,
        })
    }
}

fn spawn_writer_bridge(
    unix_read: std::os::unix::net::UnixStream,
    mut input: Pin<Box<dyn tokio::io::AsyncWrite + Send>>,
) -> Result<()> {
    unix_read
        .set_nonblocking(true)
        .context("Failed to set Unix socket to non-blocking")?;
    let mut async_read = tokio::net::UnixStream::from_std(unix_read)
        .context("Failed to convert Unix socket to tokio")?;

    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            match async_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if input.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    if input.flush().await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!("Arena writer bridge error: {e}");
                    break;
                }
            }
        }
        tracing::debug!("Arena writer bridge task exiting");
    });

    Ok(())
}

fn spawn_reader_bridge(
    scenario_prompt: Option<String>,
    mut output: OutputStream,
    sender: SyncSender<Vec<u8>>,
    stop_flag: Arc<AtomicBool>,
) {
    tokio::spawn(async move {
        // Inject scenario prompt before live stream
        if let Some(prompt) = scenario_prompt {
            if sender.send(prompt.into_bytes()).is_err() {
                return;
            }
        }

        while let Some(chunk) = output.next().await {
            if stop_flag.load(Ordering::Acquire) {
                break;
            }
            match chunk {
                Ok(log_output) => {
                    let bytes = match log_output {
                        LogOutput::Console { message }
                        | LogOutput::StdOut { message }
                        | LogOutput::StdErr { message } => message,
                        _ => continue,
                    };
                    if sender.send(bytes.to_vec()).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!("Arena output stream error: {e}");
                    break;
                }
            }
        }
        tracing::debug!("Arena output reader task exiting");
    });
}

#[async_trait]
impl PtySession for ArenaPtySession {
    async fn take_reader(
        &mut self,
        sender: SyncSender<Vec<u8>>,
    ) -> Result<PtyReader> {
        let output = self
            .output
            .lock()
            .expect("output lock poisoned")
            .take()
            .context("Output stream already taken")?;

        let stop_flag = Arc::new(AtomicBool::new(false));
        spawn_reader_bridge(
            self.scenario_prompt.take(),
            output,
            sender,
            stop_flag.clone(),
        );

        Ok(PtyReader::with_stop_flag(stop_flag))
    }

    async fn take_writer(&mut self) -> Result<Arc<PtyWriter>> {
        let unix_write = self
            .writer_handle
            .take()
            .context("Writer already taken - can only be called once")?;

        Ok(Arc::new(PtyWriter::new(Box::new(unix_write))))
    }

    async fn resize(&self, size: PtySize) -> Result<()> {
        self.container
            .as_ref()
            .context("Container already stopped")?
            .resize(size.cols, size.rows)
            .await
            .context("Failed to resize arena container TTY")
    }

    fn send_sigint(&self) -> Result<()> {
        use std::io::Write as _;
        let mut writer = self
            .sigint_handle
            .lock()
            .expect("sigint handle lock poisoned");
        writer
            .write_all(&[0x03])
            .context("Failed to send Ctrl+C to arena container")
    }

    async fn is_running(&self) -> bool {
        match self.container.as_ref() {
            Some(container) => container.is_running().await,
            None => false,
        }
    }

    async fn kill(&self) -> Result<()> {
        self.container
            .as_ref()
            .context("Container already stopped")?
            .stop()
            .await
            .context("Failed to stop arena container")
    }
}
```

**Step 2: Export ArenaPtySession from adapters**

In `src/pty/adapters.rs`, add the pub use:

```rust
#[cfg(feature = "arena")]
mod arena;

#[cfg(feature = "arena")]
pub use self::arena::ArenaPtySession;
```

**Step 3: Wire PtyManager::new()**

In `src/pty/manager.rs`, replace the `todo!()` placeholder with the real implementation:

```rust
#[cfg(feature = "arena")]
PtyProvider::ArenaScenario(ref image) => (
    Box::new(super::adapters::ArenaPtySession::new(image).await?)
        as Box<dyn PtySession>,
    "arena".to_string(),
),
```

**Step 4: Verify it compiles**

Run: `cargo build --features arena`
Expected: Compiles successfully.

Run: `cargo build`
Expected: Compiles successfully (arena gated out).

**Step 5: Run all tests**

Run: `cargo test --all-features`
Expected: All tests pass.

**Step 6: Commit**

```bash
git add -A
git commit -m "feat(arena): add ArenaPtySession with scenario prompt injection"
```

---

### Task 6: Integration test with a real Docker image

**Files:**
- Create: `tests/arena_integration.rs` (or manual test)

**Step 1: Manual integration test**

Build and run with a test image (e.g., `debian:bookworm-slim` which won't have `/arena/scenario.json` but should still work as a PTY):

Run: `cargo run --features arena -- --arena debian:bookworm-slim`
Expected:
- Docker image is pulled (if not cached)
- Terminal opens with a bash prompt inside the container
- You can type commands (`ls`, `whoami`, `cat /etc/os-release`)
- A warning is logged about missing `/arena/scenario.json` (expected)
- Closing the tab stops and removes the container

**Step 2: Test with a scenario manifest**

Create a quick test Dockerfile:

```dockerfile
FROM debian:bookworm-slim
RUN mkdir -p /arena && echo '{"title":"Test Scenario","prompt":{"title":"INCIDENT ALERT","body":"Something is broken","environment":"test-env","mission":"Find the root cause"}}' > /arena/scenario.json
```

Build: `docker build -t arena-test:latest -f /tmp/Dockerfile.arena-test /tmp`
Run: `cargo run --features arena -- --arena arena-test:latest`
Expected:
- Terminal opens and shows the formatted incident prompt before the shell prompt
- Normal terminal interaction works after the prompt

**Step 3: Verify container cleanup**

After closing the app, run: `docker ps -a | grep infraware_arena`
Expected: No containers remain.

**Step 4: Run clippy**

Run: `cargo clippy --all-features -- -D warnings`
Expected: No warnings.

**Step 5: Format**

Run: `cargo +nightly fmt --all`

**Step 6: Commit**

```bash
git add -A
git commit -m "chore(arena): integration test verification and cleanup"
```

---

## Summary

| Task | Description | Key Files |
|------|-------------|-----------|
| 1 | Remove SIM_FIXTURE code | `shell.rs`, `Cargo.toml` |
| 2 | Add `arena` feature + CLI arg | `Cargo.toml`, `args.rs`, `main.rs`, `manager.rs` |
| 3 | ScenarioManifest struct | `arena/scenario.rs` |
| 4 | ArenaContainer (Docker setup) | `arena/container.rs` |
| 5 | ArenaPtySession (full adapter) | `arena/mod.rs`, `adapters.rs`, `manager.rs` |
| 6 | Integration testing | Manual Docker test |
