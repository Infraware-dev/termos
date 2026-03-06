# Arena Mode Design

Date: 2026-03-06

## Overview

Arena Mode is a competitive incident investigation challenge mode for the Infraware terminal.
Users connect to a Docker container preconfigured with a broken environment and investigate to
find the root cause — like a CTF for SRE/DevOps.

Arena is implemented as a new PTY backend adapter (`ArenaPtySession`), meaning it is invisible
to the UI, state machine, agent, and HITL layers. It's just another tab backed by a container.

## CLI & Feature Flag

- New Cargo feature: `arena` (depends on `bollard`)
- New clap CLI flag: `--arena <IMAGE>` (gated behind `#[cfg(feature = "arena")]`)
- When `--arena` is provided, the app opens a tab with `PtyProvider::ArenaScenario(image)` instead of `PtyProvider::Local`

`PtyProvider` gains a new variant:

```rust
#[cfg(feature = "arena")]
ArenaScenario(String),  // Docker image reference
```

## ArenaPtySession Adapter

New module at `src/pty/adapters/arena/`, gated behind `#[cfg(feature = "arena")]`:

```
src/pty/adapters/arena/
  mod.rs          -- ArenaPtySession struct + PtySession impl
  container.rs    -- ArenaContainer (Docker setup, image pull, lifecycle)
  scenario.rs     -- ScenarioManifest struct
```

### ArenaPtySession

Forked from `TestContainerPtySession`. Same Unix socket bridging pattern for async-to-sync I/O.

Differences from TestContainerPtySession:
- Takes an `image: String` parameter (no hardcoded image)
- After container attach, reads `/arena/scenario.json` via `docker exec`
- Prints the scenario prompt (incident alert) to the reader channel before live PTY stream
- Stores the parsed `ScenarioManifest` for future use (scoring)

### ArenaContainer

Forked from `Container` in `test_container/container.rs`. Same setup flow
(pull -> create -> start -> attach) but parameterized on image name.

### ScenarioManifest

Minimal struct, read from `/arena/scenario.json` inside the container:

```rust
#[derive(Debug, Deserialize)]
pub struct ScenarioManifest {
    pub title: String,
    pub prompt: ScenarioPrompt,
}

#[derive(Debug, Deserialize)]
pub struct ScenarioPrompt {
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub mission: Option<String>,
}
```

More fields (scoring, difficulty, etc.) will be added when needed.

## Integration & Wiring

- `PtyManager::new()`: New match arm for `ArenaScenario(image)` that constructs `ArenaPtySession`
- `src/main.rs`: When `args.arena` is Some, use `PtyProvider::ArenaScenario(image)`
- `src/pty/adapters.rs`: Declare `arena` module behind feature gate, re-export
- `src/pty.rs`: Re-export `ArenaPtySession` in public API behind feature gate

No changes to: `app.rs`, `session.rs`, `state.rs`, input handling, HITL flow,
agent orchestrator, or any UI code.

## Cleanup: Remove SIM_FIXTURE

In `src/agent/adapters/rig/shell.rs`:
- Delete `SimFixture`, `SimEntry`, `default_fallback()`, `simulate_command()`
- Remove `SIM_FIXTURE` env var check from `spawn_command()`
- Delete all sim-related tests
- Remove unused imports (`regex::Regex`, `serde::Deserialize`)

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Scenario environment | Real Docker container | Total realism, no regex gaps, any command works |
| Metadata location | `/arena/scenario.json` inside image | Self-contained, single artifact per scenario |
| Filesystem restrictions | Up to scenario author | Some scenarios may want read-only, others allow writes |
| Agent command execution | All through PTY | Same as normal mode, commands run inside the container |
| Adapter reuse | Fork TestContainerPtySession | Arena will diverge (scoring, etc.) and be removed later |
| UI | No special chrome | Arena is just another PTY tab, no overlays or timers |
| Feature gating | `--features arena` | Users who don't need arena don't pay for it |

## Future Work (Not In Scope)

- Scoring and root cause submission
- Leaderboard API
- Timer and command count overlay
- Scenario browsing UI
- Share cards
- Variant system (per-user image tags)
