# Interactive Commands Architecture

## Overview

This document describes the architecture and design of the interactive command system in Infraware Terminal. Interactive commands (vim, less, top, etc.) require special handling because they need full terminal control and cannot have their output captured by the TUI.

**Key Design Challenge**: How to suspend the TUI, give the user full terminal access, and reliably restore the TUI afterward—even if the command panics.

## Key Concepts

### Interactive vs Non-Interactive Commands

**Interactive Commands** (REQUIRES_INTERACTIVE):
- Text editors: vim, nvim, nano, emacs
- Pagers: less, more, man
- File managers: mc, ranger, nnn, lf
- System monitors: top, htop, btop, atop
- Privilege escalation: sudo

**Blocked Interactive Commands** (INTERACTIVE_BLOCKED):
- Remote access: ssh, tmux, screen, ftp, sftp
- REPLs: python, node, irb, psql, mysql
- Debuggers: gdb, lldb, pdb
- Admin tools: passwd, visudo
- Root-only monitors: iotop, iftop, nethogs

**Non-Interactive Commands**:
- Everything else (ls, grep, echo, apt-get, etc.)
- Output is captured and displayed in the TUI

### Platform Support

- **Unix/Linux/macOS**: Full interactive command support via TUI suspension
- **Windows**: Interactive commands return error (requires Windows Terminal API implementation)

## Architecture Components

### 1. CommandExecutor (src/executor/command.rs)

Responsible for actual command execution with two distinct methods:

#### `execute(cmd, args, original_input) -> Result<CommandOutput>`

Non-interactive command execution with output capture.

**Execution Paths**:
1. Check INTERACTIVE_BLOCKED list → Return error with suggestions
2. Shell builtin? → Execute via `sh -c "cmd args"`
3. Shell operators (pipes, redirects)? → Execute via `sh -c "full_command"`
4. Regular command? → Direct execution

**Output**: Full CommandOutput with captured stdout/stderr

**Timeout**: 5 minutes per command

**Example**:
```rust
CommandExecutor::execute("ls", &["-la".to_string()], None)
  -> CommandOutput { stdout: "...", stderr: "", exit_code: 0 }
```

#### `execute_interactive(cmd, args, ui) -> Result<CommandOutput>`

Interactive command execution with TUI suspension.

**Execution Steps**:
1. Suspend TUI (show_cursor, flush, leave alternate screen, disable raw mode)
2. Run command via `tokio::spawn_blocking()` (unguarded process execution)
3. Resume TUI (enable raw mode, enter alternate screen, clear)
4. Return exit code only (no output captured)

**Safety**: RAII Guard ensures resume() is called even on panic

**Example**:
```rust
CommandExecutor::execute_interactive("vim", &["file.txt".to_string()], ui)
  -> CommandOutput { stdout: "", stderr: "", exit_code: 0 }
  // User edits file, TUI returns intact
```

### 2. CommandOrchestrator (src/orchestrators/command.rs)

Routes commands to appropriate execution method based on interactivity.

**Responsibilities**:
- Handle built-in commands (clear, reload-aliases)
- Verify command existence
- Route to interactive or non-interactive execution
- Format and display output
- Provide user-friendly error messages

**Decision Flow**:
```
Input Command
  ↓
Is built-in? (clear, reload-aliases)
  ├─ Yes → Handle directly
  └─ No → Check existence
           ↓
           Does command require interactive?
           ├─ Yes → execute_interactive(cmd, args, ui)
           └─ No → execute(cmd, args, original_input)
                   ↓
                   Capture and format output
```

**Output Handling**:
- Display stdout as-is
- Display stderr (colorize red if command failed)
- Skip error message for benign failures (exit code 1)
- Show "Command failed: X" for real errors (exit code 2+)

### 3. TerminalUI (src/terminal/tui.rs)

Manages terminal state and TUI lifecycle.

#### Key Methods

**`suspend()`**: Prepare terminal for interactive command
1. `show_cursor()` - Make cursor visible
2. `flush()` - Ensure pending draws complete
3. `LeaveAlternateScreen` - Show normal terminal buffer
4. `disable_raw_mode()` - Restore line buffering and echo

Result: User sees normal terminal, can interact with vim/less/etc.

**`resume()`**: Restore TUI after interactive command
1. `enable_raw_mode()` - Disable line buffering and echo
2. `EnterAlternateScreen` - Switch to alternate screen buffer
3. `clear()` - Prevent visual artifacts

Result: TUI restored, ready for next input

#### RAII Guard Pattern

```rust
struct TuiGuard<'a> {
    ui: &'a mut TerminalUI,
    suspended: bool,
}

impl<'a> Drop for TuiGuard<'a> {
    fn drop(&mut self) {
        if self.suspended {
            let _ = self.ui.resume();  // Guaranteed to run on panic
        }
    }
}
```

**Benefits**:
- Automatic cleanup even on panic
- No manual resume() calls needed
- Terminal always restored

### 4. TerminalState (src/terminal/state.rs)

Manages TUI state with SRP-compliant components.

**Components**:
- `OutputBuffer`: Scrollable output (max 10,000 lines)
- `InputBuffer`: User input with cursor positioning
- `CommandHistory`: Command history with navigation
- `TerminalMode`: Current operation mode

**TerminalMode Enum**:
```rust
pub enum TerminalMode {
    Normal,           // Waiting for input
    ExecutingCommand, // Running shell command
    WaitingLLM,       // Querying LLM
    PromptingInstall, // Asking to install command
}
```

## Data Flow

### Complete Interactive Command Flow

```
User Input "vim file.txt"
  ↓
InputClassifier::classify()
  - Expand aliases
  - Run 9-handler chain
  - Return InputType::Command
  ↓
InfrawareTerminal::handle_command()
  ↓
CommandOrchestrator::handle_command()
  - Verify command exists
  - Check requires_interactive("vim") → true
  ↓
CommandExecutor::execute_interactive(ui)
  ↓
  TuiGuard created (suspended=false)
    ↓
    TerminalUI::suspend()
      - show_cursor()
      - flush()
      - LeaveAlternateScreen
      - disable_raw_mode()
    ↓
    TuiGuard.suspended = true
    ↓
    tokio::spawn_blocking()
      - std::process::Command::new("vim")
      - .arg("file.txt")
      - .status()
      ↓
      [User edits file in vim]
      ↓
      Returns exit code
    ↓
    TerminalUI::resume()
      - enable_raw_mode()
      - EnterAlternateScreen
      - clear()
  ↓
  TuiGuard dropped
    - If panic occurred, Drop ensures resume() was called
    - Sets suspended = false
  ↓
CommandOutput { exit_code: 0, stdout: "", stderr: "" }
  ↓
Display "Interactive command 'vim' completed successfully"
  ↓
Render updated TUI with previous state
```

### Complete Non-Interactive Command Flow

```
User Input "ls -la"
  ↓
InputClassifier::classify()
  - Return InputType::Command
  ↓
InfrawareTerminal::handle_command()
  ↓
CommandOrchestrator::handle_command()
  - Verify command exists
  - Check requires_interactive("ls") → false
  ↓
CommandExecutor::execute()
  - Check INTERACTIVE_BLOCKED
  - Not a shell builtin
  - No shell operators
  - Direct execution
    ↓
    TokioCommand::new("ls")
      .args(["-la"])
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .output()
      ↓
      [System runs ls]
      ↓
      Returns output + exit code
  ↓
CommandOutput { stdout: "...", stderr: "", exit_code: 0 }
  ↓
CommandOrchestrator::execute_and_display()
  - Display stdout lines
  - Display stderr (if any)
  - Check exit code (benign vs error)
  ↓
TerminalState updated with output lines
  ↓
Render updated TUI with new output
```

## Key Design Decisions

### 1. TUI Suspension Strategy

**Why suspend/resume?**
- Interactive commands need full terminal control
- Cannot have output captured (would go to TUI buffer)
- Need normal line editing, cursor, echo for password prompts
- User expects full terminal behavior

**How it works?**
- Leave alternate screen buffer → User sees normal terminal
- Disable raw mode → Terminal handles line editing automatically
- Command runs unguarded in `spawn_blocking()`
- RAII Guard ensures restore even on panic

### 2. RAII Guard for Safety

**Problem**: If restore isn't called, terminal is left in broken state
**Solution**: Drop trait ensures resume() always runs

```rust
impl Drop for TuiGuard<'a> {
    fn drop(&mut self) {
        if self.suspended {
            let _ = self.ui.resume();  // Always runs, even on panic
        }
    }
}
```

### 3. Platform-Specific Implementation

**Unix/Linux/macOS**: Fully supported via crossterm
**Windows**: Requires ConPTY API (deferred to M2/M3)

```rust
#[cfg(not(target_os = "windows"))]  // Unix/Linux/macOS
{
    // Full implementation
}

#[cfg(target_os = "windows")]
{
    return Ok(CommandOutput {
        stderr: "Interactive commands not supported on Windows",
        exit_code: 1,
    });
}
```

### 4. Output Handling Strategy

**Non-Interactive Output**:
- Stdout and stderr both captured
- Displayed in TUI for scrolling and search
- Color codes parsed via ansi_to_tui crate

**Interactive Output**:
- No output captured (goes directly to user's terminal)
- Only exit code captured
- Displayed as completion message

**Benign Failures**:
- Exit code 1: Often semantic (grep no match, diff differences, test false)
- Exit code 2+: Real errors requiring user notification
- Prevents false error messages for grep/diff/test

## API Reference

### CommandExecutor

```rust
impl CommandExecutor {
    // Check if command requires interactive mode
    pub fn requires_interactive(cmd: &str) -> bool

    // Non-interactive execution with output capture
    pub async fn execute(
        cmd: &str,
        args: &[String],
        original_input: Option<&str>,
    ) -> Result<CommandOutput>

    // Interactive execution with TUI suspension
    pub async fn execute_interactive(
        cmd: &str,
        args: &[String],
        ui: &mut TerminalUI,
    ) -> Result<CommandOutput>

    // Check if command exists in PATH
    pub fn command_exists(cmd: &str) -> bool
}
```

### CommandOrchestrator

```rust
impl CommandOrchestrator {
    pub async fn handle_command(
        &self,
        cmd: &str,
        args: &[String],
        original_input: Option<&str>,
        state: &mut TerminalState,
        ui: &mut TerminalUI,
    ) -> Result<()>
}
```

### TerminalUI

```rust
impl TerminalUI {
    pub fn new() -> Result<Self>
    pub fn render(&mut self, state: &TerminalState) -> Result<()>
    pub fn clear(&mut self) -> Result<()>
    pub fn suspend(&mut self) -> Result<()>
    pub fn resume(&mut self) -> Result<()>
    pub fn cleanup(&mut self) -> Result<()>
}
```

## Testing Strategy

### Unit Tests

1. **CommandExecutor tests**:
   - `test_requires_interactive()` - Verify correct classification
   - `test_is_interactive_command()` - Check blocked commands
   - `test_execute()` - Non-interactive execution
   - `test_command_exists()` - PATH verification

2. **CommandOrchestrator tests**:
   - `test_execute_and_display()` - Output capture and formatting
   - `test_benign_failure()` - Exit code 1 handling
   - `test_command_not_found()` - Error messages

### Integration Tests

1. **Interactive command flow**:
   - Cannot fully test without TTY
   - Verify UI suspend/resume calls made
   - Test RAII guard behavior

2. **Non-interactive command flow**:
   - Full end-to-end tests
   - Verify output capture and display
   - Test various exit codes and stderr handling

## Performance Characteristics

### Non-Interactive Commands
- Average execution: <100ms (command-dependent)
- Output capture: <10ms for typical 1000-line output
- Timeout: 5 minutes per command

### Interactive Commands
- No output capture overhead
- TUI suspend: <1ms (just terminal control sequences)
- TUI resume: <1ms (same)
- Restoration guaranteed even with command panic

## Known Limitations and Future Work

### Current Limitations (M1)
- Interactive commands unsupported on Windows (requires ConPTY API)
- No output capture for interactive commands (by design)
- No command-specific suspend/resume hooks

### Deferred to M2/M3
- Windows interactive command support (ConPTY implementation)
- Streaming output for long-running commands
- Custom suspend/resume hooks for special commands
- Audio feedback on command completion

## Related Documents

- `SCAN_ARCHITECTURE.md` - Input classification chain
- `CLAUDE.md` - Project guidelines and constraints
- `src/executor/command.rs` - Implementation details
- `src/orchestrators/command.rs` - Orchestrator logic
- `src/terminal/tui.rs` - TUI state management

## Diagrams

The following PlantUML diagrams visualize different aspects of this architecture:

1. **interactive_command_flow.puml** - Sequence diagram showing complete flow
2. **command_executor.puml** - Class diagram of execution components
3. **terminal_ui_architecture.puml** - TUI state and control flow
4. **command_execution_decision_tree.puml** - Decision flow in orchestrator
5. **interactive_vs_noninteractive.puml** - Detailed comparison of both paths
6. **orchestration_architecture.puml** - Complete orchestrator coordination

## Summary

The interactive command architecture solves the problem of running user-interactive commands within a TUI environment by:

1. **Detecting interactivity** early using O(1) HashSet lookup
2. **Suspending the TUI** before command execution (crossterm control sequences)
3. **Running the command unguarded** via `spawn_blocking()` (gives full terminal control)
4. **Restoring the TUI** after command completion (RAII Guard guarantees it happens)
5. **Providing user-friendly errors** for unsupported commands with suggestions

This design is **production-ready** for Unix/Linux/macOS (M1), with Windows support deferred to M2/M3 pending ConPTY API implementation.
