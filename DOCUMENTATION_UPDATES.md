# Documentation Updates: RigEngine Architecture & needs_continuation Flow

**Date**: 2026-01-16
**Status**: Complete

---

## Summary

Comprehensive documentation audit and update completed for Infraware Terminal to reflect RigEngine as the primary engine and document the `needs_continuation` flag flow that controls intelligent agent continuation after command execution.

---

## Files Updated

### 1. CLAUDE.md
- **Architecture diagram**: Updated to show RigEngine prominently
- **Engine descriptions**: RigEngine listed first as primary
- **State machine**: Added ExecutingCommand state
- **RigEngine section**: Comprehensive documentation of needs_continuation flag behavior
- **Commands section**: Added RigEngine startup command with --features rig flag

### 2. docs/BACKEND_ARCHITECTURE.md
- **RigEngine section**: Marked as "Implementato" (Completed)
- **New sequence diagram**: RigEngine HITL with needs_continuation branching logic
- **Three scenarios**: Direct answer, processing needed, question handling
- **Mermaid flowcharts**: Show tool interception, HITL approval, PTY execution, needs_continuation decision

### 3. docs/GETTING_STARTED.md
- **RigEngine quick start**: New section showing recommended setup
- **Prerequisites**: Anthropic API key requirement
- **Interactive test**: Shows state transitions (AwaitingApproval → ExecutingCommand)
- **Reorganized**: Moved MockEngine/HttpEngine to "Alternative Configurations" section

### 4. docs/RIGENGINE_PLAN.md
- **Status header**: Added "Status: COMPLETED"
- **Implementation summary**: Current state and features verified
- **Testing checklist**: Unit tests, integration, E2E, HITL, needs_continuation
- **Future enhancements**: Roadmap for next phase (state persistence, etc.)

### 5. terminal-app/docs/uml/03-state-machine.puml
- **New state**: ExecutingCommand added with full description
- **Title updated**: "Application State Machine (AppMode) with RigEngine"
- **New transitions**:
  - AwaitingApproval → ExecutingCommand (user approved)
  - ExecutingCommand → WaitingLLM (needs_continuation=true)
  - ExecutingCommand → Normal (needs_continuation=false)
- **Annotations**: Updated to explain needs_continuation branching logic

### 6. terminal-app/docs/uml/00-architecture-overview.puml
- **AppMode enum**: Added ExecutingCommand variant

### 7. terminal-app/docs/uml/README.md
- **Main heading**: Added RigEngine integration context
- **State machine docs**: Updated for ExecutingCommand state
- **New section**: "RigEngine Integration" explaining features and state machine connection
- **Diagram descriptions**: Updated to reflect RigEngine as primary

---

## Key Concepts Documented

### needs_continuation Flag
- **false**: Command output is the complete answer (return to Normal state)
- **true**: Command output is input for agent processing (continue in WaitingLLM)

### ExecutingCommand State
- Intermediate state between AwaitingApproval and final state
- Captures shell command output
- Enabled by needs_continuation decision

### HITL (Human-in-the-Loop)
- Tools intercept before execution
- User approves dangerous commands
- PromptHook mechanism explained
- Tool call sources: ShellCommandTool, AskUserTool

---

## Architecture Overview

```
User Query (? command)
    ↓
WaitingLLM
    ↓
Tool Call (RigEngine)
    ↓
AwaitingApproval (user approves)
    ↓
ExecutingCommand (runs in PTY)
    ↓
needs_continuation check
    ├─ false → Normal (final answer displayed)
    └─ true → WaitingLLM (agent continues with output)
```

---

## Files Involved in needs_continuation

- `crates/backend-engine/src/adapters/rig/tools/shell.rs` - Shell command tool with flag
- `crates/backend-engine/src/adapters/rig/orchestrator.rs` - Tool call interception
- `crates/backend-engine/src/adapters/rig/state.rs` - Interrupt state management
- `crates/shared/src/events.rs` - Interrupt enum definition
- `terminal-app/src/state.rs` - AppMode state machine

---

## Verification

All documentation changes verified against actual implementation:
- RigEngine located at `crates/backend-engine/src/adapters/rig/`
- ExecutingCommand state in `terminal-app/src/state.rs` line 37
- needs_continuation flag in ShellCommandTool parameters
- Mermaid diagrams compatible with GitHub rendering
- PlantUML diagrams compatible with standard viewers

---

## Impact

- **Primary engine** clearly documented (RigEngine first)
- **Setup path** simplified (quick start with RigEngine)
- **State machine** complete and accurate (includes ExecutingCommand)
- **needs_continuation** logic explained with real examples
- **Cross-file consistency** maintained across all documentation

