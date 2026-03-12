# Incident Investigation Pipeline

The incident investigation pipeline is a multi-agent system that helps operators investigate production incidents. It combines human-in-the-loop (HITL) approval with structured LLM reasoning to collect evidence, analyze root causes, and produce post-mortem reports.

## How It Works

The pipeline is triggered when the user describes an incident through the normal agent (e.g., `? investigate: our payments API is returning 502 errors`). The normal agent recognizes this as an incident and calls `start_incident_investigation`, which prompts the operator to confirm before launching the pipeline.

The investigation runs in three sequential phases:

```
┌─────────────────────────────────────────────────────────────┐
│                  Incident Investigation Pipeline            │
│                                                             │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐   │
│  │  Phase 1:    │    │  Phase 2:    │    │  Phase 3:    │   │
│  │ Investigator │───►│   Analyst    │───►│  Reporter    │   │
│  │   (HITL)     │    │ (pure LLM)  │    │ (save tool)  │   │
│  └──────────────┘    └──────────────┘    └──────────────┘   │
│        │                                                    │
│        │  Operator approves each                            │
│        │  diagnostic command                                │
│        ▼                                                    │
│  ┌──────────────┐                                           │
│  │  Terminal    │                                           │
│  │  (PTY exec) │                                           │
│  └──────────────┘                                           │
└─────────────────────────────────────────────────────────────┘
```

### Phase 1: Investigation (InvestigatorAgent)

The investigator is a senior SRE agent that follows a structured methodology:

**Scoping (mandatory, before any commands):**
The agent first reviews the incident description and asks the operator scoping questions to understand the environment. Key questions include:

- What software/services are involved?
- What is the infrastructure? (bare metal, VM, containers, cloud)
- When did the issue start? Is it intermittent or constant?
- Were there recent changes or deployments?
- What monitoring/observability is in place?
- What has already been tried?

The agent asks at minimum 2-3 questions before running any diagnostic command, skipping questions already answered by the incident description.

**Diagnosis:**
After scoping, the agent runs diagnostic commands with mandatory first steps:

1. **Service status** -- process status, listening ports, health endpoints
2. **Configuration review** -- reads config files, checks for misconfigurations

Then guided investigation based on what was learned during scoping:

- Error logs (service, application, system)
- Upstream/backend health
- Resource utilization (CPU, memory, disk, connections)
- Network and connectivity (firewall, DNS, TLS)
- Recent changes on disk
- Dependency health (databases, caches, queues)

**Proactive questioning:**
During diagnosis, the agent asks the operator whenever it encounters ambiguity -- multiple config files, unexpected architecture, findings suggesting multiple causes, or access issues.

**HITL approval:**
Every diagnostic command requires operator approval before execution. The approval prompt shows:

```
Motivation: Check nginx upstream configuration
Risk: Low | Expected: Backend server addresses and health check settings
```

**Tools available:**
- `execute_diagnostic_command` -- run shell commands with motivation, risk level, and expected diagnostic value
- `ask_user` -- ask the operator questions

**Safety guards:**
- Maximum 50 diagnostic commands per investigation
- Duplicate command detection -- if the agent requests a command it already ran, the pipeline forces analysis
- Risk levels: Low (read-only), Medium (restarts, config reads), High (mutations, deletions)

### Phase 2: Analysis (AnalystAgent)

A pure LLM agent (no tools) that receives all collected evidence and produces a structured JSON analysis:

```json
{
  "root_cause": "...",
  "impact": "...",
  "affected_services": ["..."],
  "timeline": [{"timestamp": "...", "description": "..."}],
  "fix_plan": ["Step 1: ...", "Step 2: ..."]
}
```

### Phase 3: Reporting (ReporterAgent)

Writes a Markdown post-mortem report and saves it to `.infraware/incidents/<date>-<slug>.md` using the `save_incident_report` tool. The report includes:

- Summary
- Timeline
- Root Cause
- Impact
- Evidence
- Fix Plan
- Lessons Learned

## Usage

Start an investigation through the terminal's natural language interface:

```
? investigate: our web server is returning 502 errors since this morning
```

The agent will:
1. Ask you to confirm the investigation
2. Ask scoping questions about your environment
3. Propose diagnostic commands for your approval
4. Analyze collected evidence
5. Save a post-mortem report to `.infraware/incidents/`

## Files

| File | Purpose |
|------|---------|
| `src/agent/adapters/rig/incident.rs` | Pipeline orchestration, entry points, phase sequencing |
| `src/agent/adapters/rig/incident/agents.rs` | Agent builders and system prompts for all three phases |
| `src/agent/adapters/rig/incident/context.rs` | `IncidentContext`, `CommandResult`, `Finding`, `RiskLevel` data models |
| `src/agent/adapters/rig/tools/diagnostic_command.rs` | `DiagnosticCommandTool` (investigation-only) |
| `src/agent/adapters/rig/tools/start_incident.rs` | `StartIncidentInvestigationTool` |
| `src/agent/adapters/rig/tools/ask_user.rs` | `AskUserTool` (shared with normal agent) |

## Configuration

The investigation pipeline uses the same configuration as the normal RigEngine agent (see `ANTHROPIC_API_KEY`, `ANTHROPIC_MODEL`, `RIG_MAX_TOKENS`, etc.). The memory system is shared across all agents in the pipeline -- facts learned during investigation are available in future sessions.
