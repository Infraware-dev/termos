# Arena Mode

Arena Mode is a competitive incident investigation challenge mode for the Infraware terminal. Users connect to a Docker container preconfigured with a broken environment and investigate to find the root cause — like a CTF for SRE/DevOps.

## How It Works

Arena mode launches a user-specified Docker image as a PTY backend. On startup, the adapter reads a scenario manifest from `/arena/scenario.json` inside the container and displays the incident prompt in the terminal before handing off to the live shell. From that point, the user (and the AI agent) interact with the container exactly as they would with any other terminal session.

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
         ┌─────────────────────┼────────────────────────┐
         │                     │                         │
┌────────▼────────┐ ┌─────────▼──────────┐ ┌────────────▼────────────┐
│ LocalPtySession │ │TestContainerPty    │ │  ArenaPtySession        │
│ (default)       │ │Session             │ │  (arena feature)        │
│                 │ │(pty-test_container)│ │                         │
│ Host shell      │ │ Debian container   │ │  User-specified image   │
└─────────────────┘ └────────────────────┘ │  + scenario manifest    │
                                           └─────────────────────────┘
```

Arena is implemented as a `PtySession` adapter (`ArenaPtySession`), meaning it is invisible to the UI, state machine, agent, and HITL layers. It's just another tab backed by a Docker container.

## Prerequisites

- Docker daemon running and accessible
- The `arena` Cargo feature enabled

## Usage

```bash
# Run with an arena scenario image
cargo run --features arena -- --arena <IMAGE>

# Example
cargo run --features arena -- --arena infraware/arena-the-cascade:latest
```

When the `--arena` flag is provided, the app opens a single tab backed by `ArenaPtySession` instead of the default local PTY.

## Startup Sequence

1. Pull the specified Docker image (if not cached)
2. Create and start a container with TTY enabled
3. Attach to the container's stdin/stdout/stderr
4. Read `/arena/scenario.json` from inside the container via `docker exec`
5. Format and inject the incident prompt into the terminal output
6. Hand off to the live PTY stream (normal interactive shell)

If the scenario manifest is missing or malformed, the session still starts — a warning is logged and the user gets a plain shell prompt.

## Scenario Manifest

Each arena image must include a JSON file at `/arena/scenario.json` with the following structure:

```json
{
  "title": "The Cascade",
  "prompt": {
    "title": "INCIDENT ALERT - Priority: High",
    "body": "Checkout Success Rate dropped below 85% in the last 15 minutes. Multiple services are reporting elevated error rates.",
    "environment": "Kubernetes cluster prod-eu-west-1, 3 namespaces, 12 services",
    "mission": "Identify the root cause and determine which component initiated the failure cascade"
  }
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `title` | Yes | Scenario name (used in logs) |
| `prompt.title` | Yes | Alert headline displayed prominently |
| `prompt.body` | Yes | Incident description |
| `prompt.environment` | No | Environment details |
| `prompt.mission` | No | User's objective |

The prompt is rendered with ANSI formatting: the title appears in bold red, and optional fields are labeled and bolded.

## Building a Scenario Image

A minimal scenario image needs:

1. A shell (e.g., bash)
2. The broken environment to investigate (misconfigured services, corrupted files, etc.)
3. `/arena/scenario.json` with the incident prompt

Example Dockerfile:

```dockerfile
FROM debian:bookworm-slim

# Install tools the investigator might need
RUN apt-get update && apt-get install -y \
    curl net-tools procps vim less \
    && rm -rf /var/lib/apt/lists/*

# Set up the broken environment
COPY setup-broken-env.sh /tmp/
RUN /tmp/setup-broken-env.sh && rm /tmp/setup-broken-env.sh

# Add the scenario manifest
COPY scenario.json /arena/scenario.json

CMD ["/bin/bash"]
```

## Container Lifecycle

- Containers are named `infraware_arena_<uuid>` for easy identification
- When the terminal tab is closed or the app exits, the container is stopped and removed
- The `Drop` implementation ensures cleanup even on unexpected exits

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Scenario environment | Real Docker container | Total realism — any command works, no simulation gaps |
| Metadata location | `/arena/scenario.json` inside image | Self-contained, single artifact per scenario |
| Agent command execution | All through PTY | Same as normal mode, commands run inside the container |
| Adapter approach | Fork of TestContainerPtySession | Arena will diverge (scoring, variants) over time |
| UI treatment | No special chrome | Arena is just another PTY tab, no overlays or timers |
| Feature gating | `--features arena` | Users who don't need arena don't pay for the dependency |

## File Structure

```
src/pty/adapters/arena/
  mod.rs          -- ArenaPtySession struct + PtySession impl
  container.rs    -- ArenaContainer (Docker lifecycle, image pull, exec)
  scenario.rs     -- ScenarioManifest and ScenarioPrompt structs
```

## Configuration

| Parameter | CLI Flag | Default |
|-----------|----------|---------|
| Arena image | `--arena <IMAGE>` | *(none — arena mode is opt-in)* |

## Feature Flags

| Feature | Dependencies | Description |
|---------|-------------|-------------|
| `arena` | `bollard` | Arena mode Docker container backend |

## Future Work

These are planned but not yet implemented:

- Scoring and root cause submission
- Leaderboard API
- Timer and command count overlay
- Scenario browsing UI
- Share cards for completed challenges
- Variant system (per-user randomized image tags)
