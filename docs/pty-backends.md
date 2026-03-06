# PTY Backends

Infraware Terminal uses a pluggable PTY (Pseudo-Terminal) backend system. Each terminal session runs on a PTY backend that provides interactive shell access — either on the local host or inside a Docker container.

## Architecture

```
                    ┌──────────────────────┐
                    │   TerminalSession    │
                    │                      │
                    │  PtyReader / PtyWriter│
                    └──────────┬───────────┘
                               │
                    ┌──────────▼───────────┐
                    │     PtyManager       │
                    │  Box<dyn PtySession> │
                    └──────────┬───────────┘
                               │
              ┌────────────────┼────────────────┐
              │                                 │
   ┌──────────▼──────────┐          ┌───────────▼───────────┐
   │  LocalPtySession    │          │ TestContainerPtySession│
   │  (portable-pty)     │          │ (bollard / Docker)     │
   │                     │          │                        │
   │  Host shell process │          │  Debian container      │
   └─────────────────────┘          └────────────────────────┘
```

All backends implement the `PtySession` trait, which provides:

- **`take_reader`** — streams output bytes to a sync channel
- **`take_writer`** — returns a writer handle for sending input
- **`resize`** — changes terminal dimensions
- **`send_sigint`** — sends Ctrl+C / SIGINT
- **`is_running`** — checks if the session process is alive
- **`kill`** — terminates the session

## Available Backends

### Local (default)

The local backend spawns a shell process on the host system using [`portable-pty`](https://docs.rs/portable-pty/). This is the default and requires no additional setup.

- Detects the best available shell: **zsh** > **bash** > **sh**
- Spawns an interactive shell (`-i` flag)
- Inherits the parent process environment
- Sets `TERM=xterm-256color` for full terminal emulation

**Usage:**

```bash
cargo run
```

### Test Container (Docker)

The test container backend runs an isolated Debian container via the Docker API using [`bollard`](https://docs.rs/bollard/). This is useful for:

- **Sandboxed command execution** — the AI agent can run commands without affecting the host
- **Consistent testing environment** — always a clean Debian system
- **Security** — commands execute inside a disposable container

The container uses the `debian:bookworm-slim` image and runs `/bin/bash` with TTY enabled.

**Prerequisites:**

- Docker daemon running and accessible (via Unix socket or TCP)
- The `pty-test_container` Cargo feature enabled

**Usage:**

```bash
# Build with the test container feature
cargo run --features pty-test_container -- --use-pty-test-container

# Or via environment variable
USE_PTY_TEST_CONTAINER=true cargo run --features pty-test_container
```

**How it works:**

The adapter bridges Docker's async I/O to the sync `PtyReader`/`PtyWriter` types using Unix socket pairs and tokio tasks:

```
Writer path:
  PtyWriter → Unix socket (sync) → tokio task → bollard stdin (async)

Reader path:
  bollard stdout (async) → tokio task → SyncSender → PtyReader consumer
```

On startup the adapter:

1. Pulls `debian:bookworm-slim` (if not cached)
2. Creates a container with `tty=true`, `open_stdin=true`, `cmd=[/bin/bash]`
3. Starts the container
4. Attaches to stdin/stdout/stderr streams
5. Sets up async-to-sync bridges for reader and writer

When the session is killed, the container is stopped and removed.

## Configuration

| Parameter | CLI Flag | Env Variable | Default |
|-----------|----------|--------------|---------|
| PTY backend | `--use-pty-test-container` | `USE_PTY_TEST_CONTAINER` | `false` (local) |
| Log level | `--log-level` / `-l` | `RUST_LOG` or `LOG_LEVEL` | `info` |

## Feature Flags

| Feature | Dependencies | Description |
|---------|-------------|-------------|
| *(default)* | `portable-pty` | Local PTY backend (always available) |
| `pty-test_container` | `bollard` | Docker container backend |

## Adding a New Backend

1. Create a new module under `src/pty/adapters/` (e.g., `src/pty/adapters/ssh.rs` or `src/pty/adapters/ssh/`)
2. Implement the `PtySession` trait
3. Add a variant to `PtyProvider` enum in `src/pty/manager.rs`
4. Add construction logic in `PtyManager::new()`
5. Wire the CLI flag / env variable in `src/args.rs` and `src/main.rs`
6. Gate behind a feature flag if it adds optional dependencies
