# Incident Investigation: Plan & Execute Phases

**Date:** 2026-03-13
**Status:** Approved

## Summary

Extend the incident investigation pipeline with two new phases after the report is written:

1. **Planning** — A PlannerAgent generates a structured remediation plan, interactively asking the user for choices, then saves it to `.infraware/plans/`. The user reviews and can request revisions.
2. **Execution** — An ExecutorAgent takes the plan and executes it step by step, with HITL approval on every command.

## Current Flow

Investigator -> Analyst -> Reporter -> END

## New Flow

Investigator -> Analyst -> Reporter -> (HITL: create plan?) -> Planner -> (review loop) -> (HITL: execute?) -> Executor -> END

## New Incident Phases

Two new `IncidentPhase` variants:

- `Planning` — emitted when the PlannerAgent starts
- `Executing` — emitted when the ExecutorAgent starts

## New ResumeContext Variants

- `IncidentPlanConfirmation { context, analysis_text, report_path }` — gate before planning
- `IncidentExecutionConfirmation { context, plan_content, plan_path }` — gate before execution
- `IncidentPlanCommand { command, motivation, needs_continuation, risk_level, expected_diagnostic_value, plan_content, plan_path }` — HITL during execution
- `IncidentPlanQuestion { question, options, context, analysis_text, plan_content, plan_path }` — HITL questions during planning or execution

## SavePlanTool

Mirrors `SaveReportTool`:

- **Args:** `slug: String`, `content: String`
- **Result:** `saved: bool`, `path: String`, `message: String`
- Writes to `.infraware/plans/<YYYY-MM-DD>-<slug>.md`
- Same slug sanitization as `SaveReportTool`
- Not intercepted by HITL (file write)

## PlannerAgent

**Tools:** `AskUserTool`, `SavePlanTool`, `SaveMemoryTool`, `SaveSessionContextTool`

**Behavior:**
- Receives analysis output + incident context in system prompt
- Asks clarifying questions via `AskUserTool` (intercepted as `IncidentPlanQuestion`)
- Produces structured Markdown plan with numbered steps, each containing:
  - Description, exact command, risk level, expected outcome, rollback action
- Final steps must be verification commands
- Saves via `save_remediation_plan`

**Orchestration:** Same HitlHook pattern as InvestigatorAgent. `ask_user` calls intercepted and stored as `IncidentPlanQuestion`.

## Review Loop

After PlannerAgent saves the plan:

1. Read saved plan content
2. Show plan to user as assistant message
3. Ask "Would you like to change anything?" via `IncidentPlanQuestion`
4. If changes requested -> feed feedback + current plan back to PlannerAgent, loop to 1
5. If no changes -> ask "Do you want to execute this plan?" via `IncidentExecutionConfirmation`
6. Max 10 revision rounds (safety guard)

## ExecutorAgent

**Tools:** `DiagnosticCommandTool`, `AskUserTool`, `SaveMemoryTool`, `SaveSessionContextTool`

**Behavior:**
- Receives plan content in system prompt
- Executes plan step by step using `DiagnosticCommandTool` (HITL on each)
- Follows plan order
- Assesses success/failure after each step
- On failure: asks user via `AskUserTool` whether to rollback, skip, or abort
- Continues through verification steps
- Ends with execution summary

**Orchestration:** Same HITL loop as InvestigatorAgent. Uses `IncidentPlanCommand` resume context.

**Completion:** When agent returns text without tool call, all steps done. Emit `IncidentPhase::Completed` + `AgentEvent::end()`.

## File Changes

### Modified files

- `src/agent/shared/events.rs` — Add `Planning`, `Executing` to `IncidentPhase`
- `src/agent/adapters/rig/state.rs` — Add 4 new `ResumeContext` variants + `PendingInterrupt` constructors
- `src/agent/adapters/rig/incident.rs` — Extend pipeline to chain plan confirmation after report. Add planning/execution entry points.
- `src/agent/adapters/rig/incident/agents.rs` — Add `SavePlanTool`, `build_planner`, `build_executor`, prompts
- `src/agent/adapters/rig/orchestrator.rs` — Add match arms in `create_resume_stream` for new variants

No new files needed.
