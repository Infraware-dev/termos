# Multi-Agent Incident Investigation Pipeline — Design

**Date**: 2026-02-19
**Status**: Approved, ready for implementation

## Related Deep-Dive Documentation

- `docs/2026-02-20-incident-orchestrator-whitepaper.md` - Discursive technical paper with Mermaid/UML diagrams, execution algorithm, HITL logic, and failure-mode analysis.

## Context

Infraware Terminal is used for DevOps assistance in datacenters across AWS, GCP, and Azure. The goal is to
reduce incident fixing time and enable automatic post-mortem report generation via agentic AI.

**Key question answered**: rig-rs 0.28 is sufficient. A custom sequential orchestrator (~200 lines Rust)
is more appropriate than `rigs` (v0.0.8, 46% docs) or `AutoAgents` (requires architectural rework).
No new crate dependencies are needed.

## User Flow

```
? investigate ECS payments crash at 14:00
    ↓ NormalAgent (triage — existing agent)
    → calls StartIncidentInvestigationTool("ECS payments crashed")
    → launch policy:
      - auto-start on high-confidence incident signals
      - HITL confirmation on medium/low confidence
IncidentOrchestrator.run(IncidentContext)
    ↓ Phase 1: InvestigatorAgent (HITL on every command)
    ↓ Phase 2: AnalystAgent     (pure LLM, no tools)
    ↓ Phase 3: ReporterAgent    (SaveReportTool, writes Markdown)
```

No changes to `?` prefix handling. The normal LLM flow is preserved. The multi-agent pipeline activates
only when the NormalAgent detects incident investigation intent and calls the escalation tool.

## Architecture

### Three rig-rs agents

| Agent | Tools | multi_turn | Purpose |
|-------|-------|-----------|---------|
| InvestigatorAgent | DiagnosticCommandTool, AskUserTool | 20 | Collects log/metrics/events via CLI (aws, gcloud, az, kubectl) |
| AnalystAgent | none | 1 | Root cause, impact, remediation (pure LLM reasoning) |
| ReporterAgent | SaveReportTool | 1 | Writes `.infraware/incidents/YYYY-MM-DD-<slug>.md` and (P1) `.infraware/incidents/YYYY-MM-DD-<slug>.json` |

`DiagnosticCommandTool` is a dedicated tool used **only** by InvestigatorAgent. It extends the shell
command with three mandatory fields required by the operational acceptance criteria:

```rust
struct DiagnosticCommandToolInput {
    command: String,
    motivation: String,              // why this command is needed
    risk_level: RiskLevel,           // Low | Medium | High
    expected_diagnostic_value: String, // what finding we expect
    needs_continuation: bool,
}

enum RiskLevel { Low, Medium, High }
```

`ShellCommandTool` is unchanged and continues to serve the normal `?` query flow.

### Data Models

```rust
// context.rs
struct IncidentContext {
    description: String,
    started_at: DateTime<Utc>,
    commands_executed: Vec<CommandResult>,
    findings: Vec<Finding>,
}

struct CommandResult {
    command: String,
    output: String,
    motivation: String,
    risk_level: RiskLevel,              // Low | Medium | High
    expected_diagnostic_value: String,
}

struct Finding {
    command: String,
    output: String,
    significance: FindingSignificance, // High | Medium | Low
    motivation: String,
    risk_level: RiskLevel,              // Low | Medium | High
    expected_diagnostic_value: String,
}

struct AnalysisReport {
    root_cause: String,
    impact: String,
    affected_services: Vec<String>,
    timeline: Vec<TimelineEvent>,
    remediation: Vec<String>,
}
```

### Phase Events in SSE

New typed phase event variant added to `crates/shared/src/events.rs`:

```rust
enum IncidentPhaseView {
    Investigating,
    Analyzing,
    Reporting,
    Completed,
}

AgentEvent::Phase { phase: IncidentPhaseView }
```

Terminal renders phase banners:
```
🔍 Investigating incident...
🧠 Analyzing findings...
📄 Generating post-mortem report...
```

### HITL — Unchanged

InvestigatorAgent uses `DiagnosticCommandTool` → `on_tool_call()` interception →
`AgentEvent::updates_with_interrupt()` → frontend approval → `resume_run()`.

`ThreadState` in `StateStore` gains a new `IncidentPhase` field so `resume_run()` routes to the
correct agent:

```rust
enum IncidentPhase {
    Investigating(IncidentContext),
    Analyzing(IncidentContext),
    Reporting {
        ctx: IncidentContext,
        analysis: AnalysisReport,
    },
    Completed {
        incident_id: String,
        markdown_report_path: String,
        json_report_path: Option<String>,
    },
}
```

## Files to Create

| File | Content |
|------|---------|
| `crates/backend-engine/src/adapters/rig/incident/mod.rs` | `IncidentOrchestrator::run()`, phase sequencing |
| `crates/backend-engine/src/adapters/rig/incident/context.rs` | `IncidentContext`, `Finding`, `AnalysisReport` |
| `crates/backend-engine/src/adapters/rig/incident/agents.rs` | Builder functions for 3 rig agents |
| `crates/backend-engine/src/adapters/rig/tools/start_incident.rs` | `StartIncidentInvestigationTool` |
| `crates/backend-engine/src/adapters/rig/tools/diagnostic_command.rs` | `DiagnosticCommandTool` with `RiskLevel` enum |

## Files to Modify

| File | Change |
|------|--------|
| `crates/shared/src/events.rs` | Add typed phase event (`AgentEvent::Phase { phase: IncidentPhaseView }`) |
| `crates/backend-engine/src/adapters/rig/orchestrator.rs` | Register `StartIncidentInvestigationTool`; escalation routing; `ThreadState` extension |
| `crates/backend-engine/src/adapters/rig/mod.rs` | Export `incident` submodule |
| `terminal-app/src/app/llm_event_handler.rs` | Handle `AgentEvent::Phase` for phase banners |

**No changes needed**: `classifier.rs`, `input_handler.rs`, `app.rs`, `state.rs`

## Key Reuse

- `ShellCommandTool` — unchanged, continues to serve normal `?` flow only
- `AskUserTool` — reused as-is
- `on_tool_call()` HITL hook — unchanged
- `StateStore` — extended, not replaced
- `MemoryStore` — NormalAgent (triage) continues using it normally

## On rigs vs rig-rs

`rigs` (v0.0.8, 46% docs coverage) extends rig-core with `DAGWorkflow` for agent pipelines but is
too immature for production use. Revisit when it reaches v0.1+. `AutoAgents` requires architectural
rework. Custom orchestrator with raw rig-rs is the correct choice.

## Integration Addendum - Success Criteria and Rust 2024 Quality Gates

### Engineering Design Principles (mandatory)

Implementation of this plan must explicitly follow:

- SOLID principles across modules, traits, and orchestration boundaries.
- Established design patterns where appropriate (Strategy, State Machine, Factory/Builder, Adapter).
- Microsoft Rust programming/coding guidelines as a continuous baseline for architecture and code review quality.

Design implications for this pipeline:

- Single Responsibility:
  - each agent has one clear purpose (investigate, analyze, report),
  - tools remain narrowly scoped (`DiagnosticCommandTool`, `SaveReportTool`, `AskUserTool`).
- Open/Closed:
  - incident phases and tools are extensible without changing existing flow contracts.
- Liskov Substitution:
  - components behind traits must remain behaviorally compatible when swapped.
- Interface Segregation:
  - small, task-specific traits for orchestration, reporting, and tool execution.
- Dependency Inversion:
  - orchestrator depends on abstractions (traits), not concrete provider implementations.

Review gates:

- Every PR implementing this plan must include a brief SOLID/design-pattern rationale.
- Every PR must include a Rust-guidelines compliance check (or explicit justified deviations).

### Objective Alignment (locked)

The incident pipeline is considered successful only if it delivers:

1. Automatic agent launch based on detected needs:
   - **Policy (locked)**: hybrid launch policy.
     - auto-start for high-confidence incident signals,
     - HITL confirmation for medium/low confidence.
2. Explainable incident reasoning:
   - Mandatory RCA output structure:
     - Timeline
     - Root Cause
     - Fix Plan
3. Faster resolution than human-only analysis:
   - Primary KPI: MTTR reduction >= 30% versus human baseline on comparable incidents.

### Operational Acceptance Criteria

- Every proposed command includes:
  - motivation,
  - risk level (`low|medium|high`),
  - expected diagnostic value.
- HITL remains mandatory for command execution.
- Pipeline result is valid only if RCA is complete (Timeline + Root Cause + Fix Plan).
- Comparative evaluation must be run on matched incident classes (agentic vs human-only).

### Rust 2024 / Idiomatic Quality Gates

- Strong typing for incident phases:
  - avoid stringly-typed phase names in core state transitions.
- Explicit state machine transitions:
  - `Investigating -> Analyzing -> Reporting -> Completed`.
- Public boundary errors use typed enums (`thiserror`) and structured propagation.
- Timeout and cancellation handled per phase with explicit propagation.
- No long-held locks across async network/LLM calls.
- Additive, versionable SSE event schemas only (no breaking API changes).

### Delivery Priority

- P0: baseline incident pipeline + phase events + risk tagging.
- P1: structured JSON report alongside markdown + analysis/report fallback strategy.
  - fallback scope: AnalystAgent and ReporterAgent only (never command execution tools),
  - fallback trigger: timeout/provider failure,
  - fallback traceability: include fallback metadata in report timeline.
- P2: MTTR dashboarding and quality trend tracking.
