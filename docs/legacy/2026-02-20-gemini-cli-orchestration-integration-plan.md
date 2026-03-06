# Implementation Plan: Gemini CLI Orchestration Benchmark and Integration

**Date**: 2026-02-20  
**Status**: Proposed  
**Owner**: Backend/AI Platform Team

## 1. Objective

Valutare in modo sistematico i pattern di orchestrazione e multi-agent di Gemini CLI e integrare nel nostro stack solo le parti che migliorano in modo misurabile:

- MTTR
- Explainability (RCA quality)
- Safety/HITL robustness
- Maintainability (Rust 2024 + SOLID)

## 2. Expected Outcome

A fine piano avremo:

1. Un benchmark tecnico comparativo tra il nostro orchestratore incident e i pattern Gemini CLI.
2. Una decisione architetturale formale: cosa adottare, cosa scartare, cosa posticipare.
3. Un rollout incrementale in 3 milestone, con KPI e gate di qualità.

## 3. Scope

In scope:

- Routing specialistico (generalist -> specialist agent)
- Configurazione agenti dichiarativa (registry)
- Guardrail per agente (turn/timeout/tool budget)
- Fallback model policy-based per fasi non esecutive
- Modalità plan/read-only prima dell'esecuzione comandi

Out of scope (questa iniziativa):

- Riprogettazione completa del backend engine abstraction
- Parallel multi-agent execution
- Rimozione del flusso HITL attuale

## 4. Baseline Architecture (Current)

Riferimenti attuali:

- `docs/2026-02-19-incident-pipeline-design.md`
- `docs/2026-02-20-incident-orchestrator-whitepaper.md`

Flusso baseline:

1. `NormalAgent`
2. `InvestigatorAgent`
3. `AnalystAgent`
4. `ReporterAgent`

Con HITL obbligatorio su command execution.

Baseline types already present in codebase and considered stable:

- `infraware_shared::IncidentPhase` with `Investigating | Analyzing | Reporting | Completed`
- `StateStore::PendingInterrupt` + `ResumeContext` variants for incident flow
- Typed SSE `AgentEvent::Phase { phase: IncidentPhase }`

## 5. Workstreams

## 5.0 Prerequisite - Benchmark Dataset (blocking)

Before Workstream A starts, define and freeze the benchmark dataset used for KPI gates.

Dataset requirements:

- Minimum `N = 20` incidents (target 30+ when available)
- Mix of historical incidents and replay-ready synthetic scenarios
- Ground truth fields in JSONL format:
  - `description`
  - `timeline`
  - `root_cause`
  - `fix_applied`
  - `human_mttr_minutes`

Ownership and governance:

- Owner: SRE Team
- Validator: SRE Lead + Backend Lead (joint sign-off)
- Due date: must be completed before Workstream A

Acceptance:

- Dataset published in repo (or internal artifact store) with version tag
- Validation report signed by owners

## 5.1 Workstream A - Reverse Engineering Gemini CLI

Deliverable:

- Documento tecnico `docs/2026-02-20-gemini-cli-pattern-analysis.md` con:
  - pattern di orchestrazione identificati,
  - meccanismi di delega/subagents,
  - policy di model routing/fallback,
  - safety controls.

Task:

1. Mappare i componenti equivalenti (router, subagent, policy layer, tool gate).
2. Catalogare i pattern riusabili nel nostro contesto incident.
3. Evidenziare dipendenze o assunzioni non compatibili con il nostro stack.
4. Verificare licenza e vincoli legali in modo documentale.
   - Scope limit: architecture-only benchmark.
   - No direct code reuse/copy from Gemini CLI.

### Routing Decision Algorithm (design-complete section)

Milestone 1 depends on this decision. Compare and document at least these options:

1. Keyword-based deterministic routing
  - Pro: veloce, economico, ripetibile
  - Contro: recall inferiore su prompt ambigui
2. LLM classifier routing (extra turn)
  - Pro: migliore comprensione semantica
  - Contro: costo/latency maggiore, possibile nondeterminism
3. Structured enum in payload (`IncidentType`)
  - Pro: esplicito e robusto a runtime
  - Contro: richiede evoluzione API/client

Default decision for implementation kickoff:

- Start with **Option 1 (keyword-based deterministic)** + confidence score.
- If confidence below threshold, fallback to HITL question.
- Keep interface ready for Option 2 in P1 without breaking changes.

Confidence score formula (blocking before Workstream A):

- `score_raw = (w_provider * provider_match) + (w_resource * resource_match) + (w_intent * incident_intent_match) + (w_noise * noise_penalty)`
- `score = clamp(score_raw, 0.0, 1.0)`
- Proposed initial weights:
  - `w_provider = 0.40`
  - `w_resource = 0.30`
  - `w_intent = 0.30`
  - `w_noise = -0.15`
- Feature extraction rules:
  - `provider_match ∈ {0, 0.5, 1}` based on AWS/GCP/Azure explicit markers
  - `resource_match ∈ [0, 1]` based on service/entity dictionary match coverage
  - `incident_intent_match ∈ {0, 1}` for outage/degradation/error intent
  - `noise_penalty ∈ [0, 1]` for ambiguity/conflicting provider cues
- Decision thresholds (applied to clamped `score`):
  - `score >= 0.75`: auto-route to specialist
  - `0.45 <= score < 0.75`: ask HITL confirmation before routing
  - `score < 0.45`: fallback to current generic path
- Calibration requirement:
  - thresholds and weights are tuned on dataset from §5.0 and versioned in a config constant/module.

Acceptance:

- Tabella "Pattern -> Compatibilità -> Effort -> Impatto" completa.
- Decision record con algoritmo routing selezionato e fallback behavior.
- Confidence formula, thresholds, and calibration protocol documented and approved.

## 5.2 Workstream B - Gap Analysis on Our Orchestrator

Deliverable:

- Documento `docs/2026-02-20-orchestrator-gap-analysis.md`.

Task:

1. Confrontare baseline attuale con pattern Gemini CLI.
2. Identificare gap su:
  - routing specialistico,
  - configurabilità agenti,
  - fallback policy,
  - preflight planning.
3. Prioritizzare gap in P0/P1/P2.
4. Valutare i gap assumendo il routing keyword-based come da §5.1 Decision D4.
   Se la gap analysis invalida questa scelta, escalare come Decision D7.

Acceptance:

- Lista gap con owner, effort stimato, rischio e dipendenze.

## 5.3 Workstream C - Incremental Implementation

### Milestone 1 (P0): Specialist Router

Goal:

- Introdurre un router esplicito che instrada al giusto specialista (AWS/GCP/Azure/local diagnostics).

Code areas:

- `crates/backend-engine/src/adapters/rig/orchestrator.rs`
- `crates/backend-engine/src/adapters/rig/incident/agents.rs`
- `crates/backend-engine/src/adapters/rig/incident/mod.rs`
- `crates/backend-engine/src/adapters/rig/shell.rs`
- `crates/backend-engine/src/adapters/rig/state.rs`
- `crates/shared/src/events.rs` (only if additive type fields are needed)

Design notes:

- Nessuna rottura API SSE/REST.
- Routing deterministico con fallback al path attuale.
- **Safety prerequisite in M1**: plan mode minimale read-only before any command proposal.
- Router invariant: routing must never bypass HITL for command execution.

Data model changes (explicit list):

- Additive `RoutingDecision` metadata in incident context/state (internal)
- Optional `incident_type` in `IncidentContext` (internal inferred metadata, additive; NOT an API payload change in M1)
- Optional `routing_reason` for audit/debug (internal, additive)
- No breaking changes in `infraware_shared` public API during M1

State changes in `state.rs` (blocking before M1 implementation):

- Extend `ResumeContext::IncidentCommand` with additive optional routing metadata:
  - `routing_decision: Option<RoutingDecision>`
  - `routing_confidence: Option<f32>`
- Add internal struct `RoutingDecision`:
  - `provider: Option<CloudProvider>`
  - `specialist: Option<String>`
  - `reason: String`
- Define `CloudProvider` explicitly (additive enum) in `crates/backend-engine/src/adapters/rig/incident/context.rs`:
  - `enum CloudProvider { Aws, Gcp, Azure, Unknown }`
  - serde shape: `snake_case` values (`aws`, `gcp`, `azure`, `unknown`)
- Import and reuse `CloudProvider` in `state.rs` (do not duplicate enum definitions).
- Ensure constructors in `PendingInterrupt` remain source-compatible via default `None` for new optional fields.
- Keep wire-level `AgentEvent` payload backward compatible (additive fields only; no renames/removals).

Backward compatibility tests (blocking before M1 implementation):

- Add serde roundtrip tests for old/new payload shapes:
  - old payload (without routing fields) must deserialize successfully
  - new payload (with routing fields) must serialize and deserialize successfully
- Add test vectors for `AgentEvent` and `PendingInterrupt` compatibility.

Acceptance:

- Test integration: query cloud-specific dispatch corretto.
- Feature flag associated: `INCIDENT_ROUTER_ENABLED` (default `false` until rollout §9).
- Regression gate (must stay green):
  - `test_resume_without_pending_interrupt`
  - `test_stream_run_*` family
  - `PendingInterrupt` tests in `state.rs`
- Blocking gate:
  - serde backward compatibility tests for additive routing fields pass.

### Milestone 2 (P1): Agent Registry + Guardrails

Goal:

- Configurazione agenti dichiarativa con limiti operativi per agente.

Code areas:

- `crates/backend-engine/src/adapters/rig/incident/agents.rs`
- nuovo modulo: `crates/backend-engine/src/adapters/rig/incident/registry.rs`
- nuovo modulo: `crates/backend-engine/src/adapters/rig/incident/guardrails.rs`
- `crates/backend-engine/src/adapters/rig/shell.rs`

Design notes:

- Registry tipizzato in Rust (eventuale supporto file-based in fase successiva).
- Guardrails minimi:
  - `max_turns`
  - `max_phase_duration`
  - `tool_allowlist`
- `shell.rs`: per-agent `max_command_timeout` guardrail overrides global config timeout.

Acceptance:

- Test unit su enforcement limiti.
- Test E2E su abort controllato per superamento limiti.
- Feature flag associated: `INCIDENT_AGENT_GUARDRAILS_ENABLED` (default `false` until rollout §9).

### Milestone 3 (P1/P2): Plan Mode + Fallback Policy

Goal:

- Aggiungere una fase preflight read-only e fallback model policy-based per Analyst/Reporter.

Code areas:

- `crates/backend-engine/src/adapters/rig/incident/mod.rs`
- `crates/backend-engine/src/adapters/rig/incident/context.rs`
- `crates/backend-engine/src/adapters/rig/orchestrator.rs`

Design notes:

- Plan mode: no command execution, solo proposta piano diagnostico.
- Fallback solo fasi non esecutive (mai command tool).
- Plan mode full version extends the minimal read-only preflight already introduced in M1.

Acceptance:

- Test E2E: plan mode produce piano senza side effects.
- Test fault-injection: fallback attivato su timeout/error provider.
- Feature flags associated:
  - `INCIDENT_PLAN_MODE_ENABLED` (default `false` until rollout §9)
  - `INCIDENT_ANALYSIS_FALLBACK_ENABLED` (default `false` until rollout §9)

## 6. Testing Strategy

Test obbligatori:

1. Unit tests:
  - router decision logic,
  - guardrail enforcement,
  - fallback activation policy.
2. Integration tests:
  - complete incident flow con HITL approve/reject,
  - specialist routing per provider cloud,
  - resume correctness con `PendingInterrupt`.
3. Regression tests:
  - flusso normale `?` non incident invariato,
  - API payload backward-compatible.
  - existing orchestrator/state tests remain green:
    - `test_resume_without_pending_interrupt`
    - `test_stream_run_*`
    - `PendingInterrupt` test suite in `state.rs`

## 7. KPIs and Success Gates

Primary KPI:

- MTTR reduction >= 30% su set incident comparabile.

Secondary KPIs:

- RCA completeness rate (Timeline + Root Cause + Fix Plan) >= 95%.
- HITL safety violations = 0.
- Pipeline completion success rate >= 98% (incidenti pilota).

KPI measurement protocol:

- Use frozen dataset from §5.0
- Compare against human baseline on matched incident classes
- Report confidence interval and sample size in KPI report
- Agent MTTR boundary: Δt from `AgentEvent::phase(Investigating)` to `AgentEvent::phase(Completed)`.

## 8. Risks and Mitigations

Risk: over-engineering rispetto al valore reale.
Mitigation: milestone gating + benchmark misurabile ad ogni fase.

Risk: regressioni sul flusso legacy.
Mitigation: regression suite obbligatoria prima di merge.

Risk: fallback introduce output incoerenti.
Mitigation: fallback limitato ad Analyst/Reporter + tracciamento esplicito in timeline.

## 9. Rollout Plan

1. Dev environment validation.
2. Internal dogfooding (incident replay dataset).
3. Limited pilot su team SRE.
4. Progressive rollout con feature flags.

Feature flags suggerite:

- `INCIDENT_ROUTER_ENABLED`
- `INCIDENT_AGENT_GUARDRAILS_ENABLED`
- `INCIDENT_PLAN_MODE_ENABLED`
- `INCIDENT_ANALYSIS_FALLBACK_ENABLED`

Defaults:

- All new flags default to `false` until corresponding milestone acceptance is met.

## 10. Deliverables Checklist

- `docs/2026-02-20-gemini-cli-pattern-analysis.md`
- `docs/2026-02-20-orchestrator-gap-analysis.md`
- PR Milestone 1 (router)
- PR Milestone 2 (registry + guardrails)
- PR Milestone 3 (plan mode + fallback)
- KPI report post-pilot

## 11. Decision Log

Decision D1:

- Adottiamo solo pattern che aumentano KPI o safety in modo verificabile.

Decision D2:

- Manteniamo HITL obbligatorio su command execution.
- Router and specialist dispatch must preserve this invariant in all branches.

Decision D3:

- Nessuna breaking change su endpoint esistenti durante questa iniziativa.

Decision D4:

- Before Workstream A: confidence score formula and thresholds are mandatory and versioned.

Decision D5:

- Before Milestone 1 implementation: serde backward compatibility tests and explicit `state.rs` additive changes are mandatory.

Decision D6:

- Legal/license verification is non-blocking because this initiative is architecture-only benchmarking (no code reuse from Gemini CLI).
- Advanced benchmark expansion and broader regression hardening are non-blocking and can be completed during Workstream A/B.
