# Supported Commands

## Overview

Infraware Terminal is a **PTY-based terminal emulator** built with egui. It runs a real shell (bash/zsh) via pseudo-terminal, meaning **all shell commands are natively supported**.

## Architecture

```
User Input → PTY (portable-pty) → Shell (bash/zsh) → Output → VTE Parser → egui Render
```

Unlike the previous SCAN-based architecture, there is no command classification or filtering. Input goes directly to the shell.

## Command Support

### Fully Supported (via PTY)

| Category | Examples |
|----------|----------|
| Shell builtins | `cd`, `export`, `alias`, `source`, `history` |
| File operations | `ls`, `cp`, `mv`, `rm`, `mkdir`, `cat`, `grep` |
| Text processing | `sed`, `awk`, `sort`, `uniq`, `cut`, `tr` |
| Process management | `ps`, `kill`, `jobs`, `bg`, `fg`, `top`, `htop` |
| Network utilities | `curl`, `wget`, `ping`, `ssh`, `scp`, `rsync` |
| Version control | `git`, `svn`, `hg` |
| Package managers | `apt`, `yum`, `dnf`, `pacman`, `brew` |
| Containers | `docker`, `docker-compose`, `kubectl`, `helm` |
| Cloud CLIs | `aws`, `az`, `gcloud`, `terraform` |
| Build tools | `make`, `cargo`, `npm`, `pip`, `maven` |
| Interactive programs | `vim`, `nano`, `less`, `man`, `python`, `node` |

### Application-Level Features

These are handled by the terminal application, not the shell:

| Feature | Shortcut | Description |
|---------|----------|-------------|
| Clear screen | `Ctrl+L` | Clears terminal output |
| Interrupt | `Ctrl+C` | Sends SIGINT to foreground process |
| EOF | `Ctrl+D` | Sends EOF to shell |
| Scroll up | `Ctrl+Shift+Up` / Mouse wheel | Scroll output buffer |
| Scroll down | `Ctrl+Shift+Down` / Mouse wheel | Scroll output buffer |
| Copy | `Ctrl+Shift+C` | Copy selected text |
| Paste | `Ctrl+Shift+V` | Paste from clipboard |

### Shell History and Editing

All readline-style editing is handled by the shell:

| Feature | Shortcut |
|---------|----------|
| Previous command | `Up` / `Ctrl+P` |
| Next command | `Down` / `Ctrl+N` |
| Beginning of line | `Ctrl+A` / `Home` |
| End of line | `Ctrl+E` / `End` |
| Delete word | `Ctrl+W` |
| Clear line | `Ctrl+U` |
| Reverse search | `Ctrl+R` |
| Tab completion | `Tab` |

## No Longer Applicable

The following concepts from the old SCAN-based architecture **no longer exist**:

- Command classification (everything goes to shell)
- "Blocked" commands (all commands work)
- "Interactive" vs "non-interactive" distinction
- Application builtins (except `clear` via Ctrl+L)
- Command caching/discovery
- LLM fallback for unknown commands

## Platform Support

| Platform | Shell |
|----------|-------|
| Linux | bash/zsh (from $SHELL) |
| macOS | bash/zsh (from $SHELL) |
| Windows | PowerShell (planned) |

## Testing

Tests are in `src/` as inline `#[cfg(test)]` modules:

```bash
cargo test              # Run all tests
cargo test -- --nocapture  # With output
```

Key test modules:
- `src/pty/io.rs` - PTY I/O tests
- `src/pty/traits.rs` - Trait implementation tests
- `src/state.rs` - State machine tests
- `src/terminal/grid.rs` - Terminal grid tests
