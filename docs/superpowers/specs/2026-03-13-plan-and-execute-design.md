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

Five new variants (split planner/executor questions to avoid ambiguity):

- `IncidentPlanConfirmation { context, analysis_text, report_path }` — gate before planning. User confirms or rejects plan creation.
- `IncidentPlannerQuestion { question, options, context, analysis_text, revision_round }` — HITL questions during planning (PlannerAgent asks clarifying questions or review loop asks for changes). Carries `revision_round` to enforce max-10 safety guard.
- `IncidentExecutionConfirmation { context, plan_content, plan_path }` — gate before execution. User confirms or rejects execution.
- `IncidentPlanCommand { command, motivation, needs_continuation, risk_level, expected_diagnostic_value, plan_content, plan_path }` — HITL during execution (per-command approval).
- `IncidentExecutorQuestion { question, options, plan_content, plan_path }` — HITL questions during execution (e.g., rollback/skip/abort on failure).

### Rejection handling

- `(Rejected, IncidentPlanConfirmation)` — "Remediation planning skipped." + end
- `(Rejected, IncidentExecutionConfirmation)` — "Plan execution skipped." + end
- `(Rejected, IncidentPlanCommand)` — "Command rejected. Plan execution stopped." + end

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
- Asks clarifying questions via `AskUserTool` (intercepted as `IncidentPlannerQuestion`)
- Produces structured Markdown plan with numbered steps, each containing:
  - Description, exact command, risk level, expected outcome, rollback action
- Final steps must be verification commands
- Saves via `save_remediation_plan`

**Orchestration:** Same HitlHook pattern as InvestigatorAgent. `ask_user` calls intercepted and stored as `IncidentPlannerQuestion`.

## Review Loop

After PlannerAgent saves the plan:

1. Read saved plan file from disk (using path returned by `SavePlanTool`)
2. Show plan to user as assistant message
3. Ask "Would you like to change anything?" via `IncidentPlannerQuestion` with options `["No, proceed to execution", "Yes, I want changes"]`
4. If changes requested -> feed user feedback + current plan content back to PlannerAgent as prompt, PlannerAgent revises and saves again (overwrites same file), loop to 1
5. If no changes -> emit `IncidentExecutionConfirmation` interrupt: "Do you want to execute this plan?"
6. Max 10 revision rounds (safety guard, enforced via `revision_round` counter in `IncidentPlannerQuestion`)

## ExecutorAgent

**Tools:** `DiagnosticCommandTool`, `AskUserTool`, `SaveMemoryTool`, `SaveSessionContextTool`

**Behavior:**
- Receives plan content in system prompt
- Executes plan step by step using `DiagnosticCommandTool` (HITL on each)
- Follows plan order
- Assesses success/failure after each step
- On failure: asks user via `AskUserTool` (intercepted as `IncidentExecutorQuestion`) whether to rollback, skip, or abort
- Continues through verification steps
- Ends with execution summary

**Orchestration:** Same HITL loop as InvestigatorAgent. Uses `IncidentPlanCommand` resume context for commands, `IncidentExecutorQuestion` for failure-handling questions.

**Completion:** When agent returns text without tool call, all steps done. Emit `IncidentPhase::Completed` + `AgentEvent::end()`.

## Reporter-to-Planner Transition

The existing `run_analysis_and_report` function currently ends with `IncidentPhase::Completed` + `AgentEvent::end()`. This changes:

1. After ReporterAgent saves the report, capture `report_path` from the reporter's response (extracted from the `SaveReportTool` result path line, same as current logic).
2. The `analysis_text` is already available as a local variable in `run_analysis_and_report`.
3. Instead of emitting `Completed` + `end()`, store an `IncidentPlanConfirmation` interrupt with `context`, `analysis_text`, and `report_path`.
4. Emit a `Question` interrupt: "Would you like to create a remediation plan to fix this issue?" with options `["Yes, create plan", "No, skip"]`.

## Entry Points in `incident.rs`

New public functions matching the existing pattern:

- `start_planning(client, config, state, thread_id, context, analysis_text, run_id, memory_ctx)` — called when user confirms plan creation. Emits `Phase(Planning)`, builds PlannerAgent, runs with HitlHook.
- `resume_planning_question(client, config, state, thread_id, question, answer, context, analysis_text, revision_round, run_id, memory_ctx)` — called when user answers a planner question. Feeds answer back to PlannerAgent.
- `start_plan_review(client, config, state, thread_id, context, plan_content, plan_path, analysis_text, revision_round, run_id, memory_ctx)` — called after PlannerAgent saves plan. Shows plan, asks for changes. If changes → re-runs PlannerAgent with feedback. If max revisions hit → proceeds to execution confirmation.
- `start_execution(client, config, state, thread_id, context, plan_content, plan_path, run_id, memory_ctx)` — called when user confirms execution. Emits `Phase(Executing)`, builds ExecutorAgent, runs with HitlHook.
- `resume_execution_command(client, config, state, thread_id, command, motivation, needs_continuation, risk_level, expected_diagnostic_value, plan_content, plan_path, run_id, timeout_secs, memory_ctx)` — called when user approves a plan command. Executes it and feeds output back to ExecutorAgent.
- `resume_execution_with_output(...)` — same as above but with pre-captured PTY output (mirrors `resume_investigation_with_output`).
- `resume_execution_question(client, config, state, thread_id, question, answer, plan_content, plan_path, run_id, memory_ctx)` — called when user answers an executor question (rollback/skip/abort).

## Match Arms in `create_resume_stream`

New arms in the orchestrator's resume match:

```
(Answer, IncidentPlanConfirmation)       -> classify y/n -> start_planning() or end
(Answer, IncidentPlannerQuestion)        -> resume_planning_question() or start_plan_review()
(Answer, IncidentExecutionConfirmation)  -> classify y/n -> start_execution() or end
(CommandOutput, IncidentPlanCommand)     -> resume_execution_with_output()
(Rejected, IncidentPlanCommand)          -> "Command rejected. Execution stopped." + end
(Answer, IncidentExecutorQuestion)       -> resume_execution_question()
```

## File Changes

### Modified files

- `src/agent/shared/events.rs` — Add `Planning`, `Executing` to `IncidentPhase`
- `src/agent/adapters/rig/state.rs` — Add 5 new `ResumeContext` variants + `PendingInterrupt` constructors
- `src/agent/adapters/rig/incident.rs` — Extend `run_analysis_and_report` to chain plan confirmation after report. Add 7 new public entry points for planning/execution.
- `src/agent/adapters/rig/incident/agents.rs` — Add `SavePlanTool`, `build_planner`, `build_executor`, `PLANNER_PROMPT`, `EXECUTOR_PROMPT`
- `src/agent/adapters/rig/orchestrator.rs` — Add 6 new match arms in `create_resume_stream`

No new files needed.
