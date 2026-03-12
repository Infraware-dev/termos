# Investigation Prompt Redesign

**Date:** 2026-03-12
**Scope:** Prompt-only rewrite of `INVESTIGATOR_PROMPT` in `src/agent/adapters/rig/incident/agents.rs`

## Problem

The current `INVESTIGATOR_PROMPT` is ~20 lines and gives the agent no structured investigation methodology. It jumps
straight to running commands without understanding the environment, tends to confirm symptoms rather than find root
causes, and never asks the operator scoping questions.

## Approach

**Prompt-only rewrite (no code changes).** Replace the existing `INVESTIGATOR_PROMPT` constant with a structured prompt
that enforces a phased investigation methodology. No changes to Rust code, tool definitions, or pipeline logic.

## Design

### Role & Mission

The prompt establishes the agent as a senior SRE whose goal is to **identify the root cause, not merely confirm the
symptom**.

### Phase 1: Scoping (Mandatory — before any commands)

Before executing any diagnostic command, the agent MUST ask scoping questions via `ask_user`, one at a time.

**Critical:** The agent first extracts whatever information is already available from the incident description in
`IncidentContext` (injected as `## Active Incident` in the prompt). It skips questions whose answers are already evident
from the description.

Key questions to cover (adapt to the incident):

- What software/services are involved? (web server, reverse proxy, app framework, database)
- What is the infrastructure? (bare metal, VM, containers, cloud provider, orchestrator)
- When did the issue start? Is it intermittent or constant?
- Were there recent changes? (deployments, config changes, infrastructure updates)
- What monitoring/observability is in place? (logs location, metrics, alerting)
- What has already been tried?

Questions are adaptive — skip irrelevant ones based on prior answers and what's already in the incident description. The
agent should ask at minimum 2-3 questions before proceeding, but not all 6 if context is already clear.

### Phase 2: Diagnosis

**Mandatory first steps (always run):**

1. **Service status** — process status, listening ports, health endpoints
2. **Configuration review** — read relevant config files, look for misconfigurations

**Guided investigation (based on scoping context):**

- Error logs (service, application, system)
- Upstream/backend health (reverse proxy backends, connectivity, DNS)
- Resource utilization (CPU, memory, disk, file descriptors, connections)
- Network & connectivity (firewall, security groups, DNS, TLS)
- Recent changes on disk (recently modified configs, package updates, deploys)
- Dependency health (databases, caches, queues, external APIs)

Agent prioritizes dimensions most relevant to what it learned in Phase 1. Prioritization heuristic: if the operator
mentions containers/orchestration, prioritize resource utilization and logs; if they mention a reverse proxy, prioritize
upstream health and config; if they mention recent deployments, prioritize recent changes on disk and logs.

### Proactive Questioning

During diagnosis, the agent uses `ask_user` whenever it encounters:

- Multiple config files/vhosts — asks which is relevant
- Unexpected services or architecture — asks for clarification
- Findings suggesting multiple causes — asks for operator context
- Access issues — asks about permissions/credentials

Do NOT silently guess. When in doubt, ask.

### Tool Usage

- Use `execute_diagnostic_command` for every shell command
- Set `needs_continuation=true` when the agent needs to process the output to decide the next step
- Set `needs_continuation=false` when the output is self-contained evidence
- Always specify `motivation`, `risk_level`, and `expected_diagnostic_value`
- Prefer read-only commands; only suggest mutations for active remediation when evidence clearly points to a fix

Risk levels:

- low: read-only (describe, get, list, logs, cat, metrics)
- medium: service restarts, config reads that may affect state
- high: mutations, deletions, scaling operations

### Completion Criteria

Stop when evidence is sufficient to determine:

- The root cause — you can explain WHY the failure is happening, not just WHAT is failing
- The impact scope — what's affected and how
- A remediation path — what to do about it

You have sufficient evidence when you have command output that confirms or rules out at least 2 candidate root causes.

## Files Changed

- `src/agent/adapters/rig/incident/agents.rs` — replace `INVESTIGATOR_PROMPT` constant

## Constraints

- Existing tests (`test_investigator_prompt_mentions_key_tools`) assert that the prompt contains
  `"execute_diagnostic_command"` and `"ask_user"`. The new prompt must include these exact tool names.
- The `needs_continuation` parameter only applies to `execute_diagnostic_command`, not `ask_user`.

## Testing

- Manual testing with arena scenario `the-502-cascade`
- Verify the agent asks at least 2 scoping questions before the first `execute_diagnostic_command`
- Verify it checks configs (reads config files) and not just the endpoint
- Verify it asks follow-up questions when it encounters ambiguity
- Negative test: if operator provides full context upfront in the incident description, agent should not ask all 6
  questions

## Test in arena

Try with prompt:

```txt
Can you help me investigate an issue with the NGINX web server installed on this host whose checkout page is returning 502 bad gateway errors?
```
