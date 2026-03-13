//! Builder functions for the three incident-investigation rig agents.
//!
//! Each function returns a configured `RigAgent` bound to the Anthropic client:
//!
//! - `build_investigator` — HITL command collection (DiagnosticCommandTool + AskUserTool)
//! - `build_analyst`      — pure LLM root-cause analysis (no tools)
//! - `build_reporter`     — post-mortem Markdown writer (SaveReportTool)

use std::sync::Arc;

use rig::client::CompletionClient as _;
use rig::completion::ToolDefinition;
use rig::providers::anthropic;
use rig::tool::Tool;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};

use super::context::IncidentContext;
use crate::agent::adapters::rig::config::RigAgentConfig;
use crate::agent::adapters::rig::memory::persistent::SaveMemoryTool;
use crate::agent::adapters::rig::memory::session_context::SaveSessionContextTool;
use crate::agent::adapters::rig::memory::{MemoryContext, Preambles};
use crate::agent::adapters::rig::orchestrator::RigAgent;
use crate::agent::adapters::rig::tools::{AskUserTool, DiagnosticCommandTool};

// ---------------------------------------------------------------------------
// SaveReportTool
// ---------------------------------------------------------------------------

/// Arguments the LLM supplies when saving the post-mortem report.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SaveReportArgs {
    /// Filename slug (e.g. "ecs-payments-crash") — will be combined with date.
    pub slug: String,
    /// Full Markdown content of the post-mortem report.
    pub content: String,
}

/// Result returned after the report is written.
#[derive(Debug, Serialize)]
pub struct SaveReportResult {
    /// Whether the file was saved successfully.
    pub saved: bool,
    /// Absolute path of the saved file.
    pub path: String,
    /// Human-readable message.
    pub message: String,
}

/// Error type for the report-save tool.
#[derive(Debug, thiserror::Error)]
pub enum SaveReportError {
    #[error("Failed to write report: {0}")]
    Io(String),
}

/// Rig Tool that writes the post-mortem Markdown to `.infraware/incidents/`.
#[derive(Debug, Clone, Default)]
pub struct SaveReportTool;

impl Tool for SaveReportTool {
    const NAME: &'static str = "save_incident_report";

    type Error = SaveReportError;
    type Args = SaveReportArgs;
    type Output = SaveReportResult;

    #[expect(
        clippy::manual_async_fn,
        reason = "rig-rs Tool trait requires impl Future return type"
    )]
    fn definition(&self, _prompt: String) -> impl Future<Output = ToolDefinition> + Send + Sync {
        async {
            ToolDefinition {
                name: Self::NAME.to_string(),
                description: "Save the completed post-mortem report as a Markdown file under \
                    .infraware/incidents/. Call this exactly once when the report is ready. \
                    Provide a short slug (e.g. 'ecs-payments-crash') and the full Markdown content."
                    .to_string(),
                parameters: serde_json::to_value(schema_for!(SaveReportArgs))
                    .expect("Failed to generate JSON schema for SaveReportArgs"),
            }
        }
    }

    #[expect(
        clippy::manual_async_fn,
        reason = "rig-rs Tool trait requires impl Future return type"
    )]
    fn call(
        &self,
        args: Self::Args,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send {
        async move {
            use tokio::fs;

            let today = chrono::Utc::now().format("%Y-%m-%d");
            let sanitized_slug: String = args
                .slug
                .trim()
                .replace(' ', "-")
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if sanitized_slug.is_empty() {
                return Err(SaveReportError::Io(
                    "slug is empty after sanitization".to_string(),
                ));
            }
            let filename = format!("{today}-{sanitized_slug}.md");
            let dir = ".infraware/incidents";

            fs::create_dir_all(dir)
                .await
                .map_err(|e| SaveReportError::Io(e.to_string()))?;

            let path = format!("{dir}/{filename}");
            fs::write(&path, &args.content)
                .await
                .map_err(|e| SaveReportError::Io(e.to_string()))?;

            Ok(SaveReportResult {
                saved: true,
                path: path.clone(),
                message: format!("Post-mortem saved to {path}"),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// SavePlanTool
// ---------------------------------------------------------------------------

/// Arguments the LLM supplies when saving the remediation plan.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SavePlanArgs {
    /// Filename slug (e.g. "nginx-config-fix") — will be combined with date.
    pub slug: String,
    /// Full Markdown content of the remediation plan.
    pub content: String,
}

/// Result returned after the plan is written.
#[derive(Debug, Serialize)]
pub struct SavePlanResult {
    /// Whether the file was saved successfully.
    pub saved: bool,
    /// Absolute path of the saved file.
    pub path: String,
    /// Human-readable message.
    pub message: String,
}

/// Error type for the plan-save tool.
#[derive(Debug, thiserror::Error)]
pub enum SavePlanError {
    #[error("Failed to write plan: {0}")]
    Io(String),
}

/// Rig Tool that writes the remediation plan Markdown to `.infraware/plans/`.
#[derive(Debug, Clone, Default)]
pub struct SavePlanTool;

impl Tool for SavePlanTool {
    const NAME: &'static str = "save_remediation_plan";

    type Error = SavePlanError;
    type Args = SavePlanArgs;
    type Output = SavePlanResult;

    #[expect(
        clippy::manual_async_fn,
        reason = "rig-rs Tool trait requires impl Future return type"
    )]
    fn definition(&self, _prompt: String) -> impl Future<Output = ToolDefinition> + Send + Sync {
        async {
            ToolDefinition {
                name: Self::NAME.to_string(),
                description: "Save the completed remediation plan as a Markdown file under \
                    .infraware/plans/. Call this exactly once when the plan is ready. \
                    Provide a short slug (e.g. 'nginx-config-fix') and the full Markdown content."
                    .to_string(),
                parameters: serde_json::to_value(schema_for!(SavePlanArgs))
                    .expect("Failed to generate JSON schema for SavePlanArgs"),
            }
        }
    }

    #[expect(
        clippy::manual_async_fn,
        reason = "rig-rs Tool trait requires impl Future return type"
    )]
    fn call(
        &self,
        args: Self::Args,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send {
        async move {
            use tokio::fs;

            let today = chrono::Utc::now().format("%Y-%m-%d");
            let sanitized_slug: String = args
                .slug
                .trim()
                .replace(' ', "-")
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if sanitized_slug.is_empty() {
                return Err(SavePlanError::Io(
                    "slug is empty after sanitization".to_string(),
                ));
            }
            let filename = format!("{today}-{sanitized_slug}.md");
            let dir = ".infraware/plans";

            fs::create_dir_all(dir)
                .await
                .map_err(|e| SavePlanError::Io(e.to_string()))?;

            let path = format!("{dir}/{filename}");
            fs::write(&path, &args.content)
                .await
                .map_err(|e| SavePlanError::Io(e.to_string()))?;

            Ok(SavePlanResult {
                saved: true,
                path: path.clone(),
                message: format!("Remediation plan saved to {path}"),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Agent builder functions
// ---------------------------------------------------------------------------

/// Build the `InvestigatorAgent`.
///
/// Equipped with `DiagnosticCommandTool` (HITL on every command),
/// `AskUserTool` for clarifying questions, and memory save tools.
/// Memory preambles are injected so the agent knows session context
/// and persistent facts discovered in prior conversations.
pub fn build_investigator(
    client: &anthropic::Client,
    config: &RigAgentConfig,
    context: &IncidentContext,
    memory_ctx: &MemoryContext,
    preambles: &Preambles,
) -> RigAgent {
    let system_prompt = format!(
        "{}\n\n## Active Incident\n\n{}",
        INVESTIGATOR_PROMPT,
        context.to_prompt_json()
    );

    client
        .agent(&config.model)
        .preamble(&system_prompt)
        .append_preamble(&preambles.memory)
        .append_preamble(&preambles.session)
        .max_tokens(config.max_tokens as u64)
        .temperature(f64::from(config.temperature))
        .tool(DiagnosticCommandTool)
        .tool(AskUserTool::new())
        .tool(SaveMemoryTool::new(Arc::clone(&memory_ctx.memory_store)))
        .tool(SaveSessionContextTool::new(Arc::clone(
            &memory_ctx.session_context_store,
        )))
        .build()
}

/// Build the `AnalystAgent`.
///
/// No tools — pure LLM reasoning over the collected evidence.
/// Memory preambles are injected as read-only context.
/// Produces a structured `AnalysisReport` JSON in its response.
pub fn build_analyst(
    client: &anthropic::Client,
    config: &RigAgentConfig,
    context: &IncidentContext,
    preambles: &Preambles,
) -> RigAgent {
    let system_prompt = format!(
        "{}\n\n## Evidence Collected\n\n{}",
        ANALYST_PROMPT,
        context.to_prompt_json()
    );

    client
        .agent(&config.model)
        .preamble(&system_prompt)
        .append_preamble(&preambles.memory)
        .append_preamble(&preambles.session)
        .max_tokens(config.max_tokens as u64)
        .temperature(f64::from(config.temperature))
        .build()
}

/// Build the `ReporterAgent`.
///
/// Equipped with `SaveReportTool` to persist the Markdown post-mortem.
/// Memory preambles are injected as read-only context.
pub fn build_reporter(
    client: &anthropic::Client,
    config: &RigAgentConfig,
    context: &IncidentContext,
    analysis_json: &str,
    preambles: &Preambles,
) -> RigAgent {
    let system_prompt = format!(
        "{}\n\n## Incident Context\n\n{}\n\n## Analysis\n\n{}",
        REPORTER_PROMPT,
        context.to_prompt_json(),
        analysis_json
    );

    client
        .agent(&config.model)
        .preamble(&system_prompt)
        .append_preamble(&preambles.memory)
        .append_preamble(&preambles.session)
        .max_tokens(config.max_tokens as u64)
        .temperature(f64::from(config.temperature))
        .tool(SaveReportTool)
        .build()
}

/// Build the `PlannerAgent`.
///
/// Equipped with `AskUserTool` for clarifying questions and `SavePlanTool`
/// to persist the remediation plan. Memory tools are also included.
pub fn build_planner(
    client: &anthropic::Client,
    config: &RigAgentConfig,
    context: &IncidentContext,
    analysis_text: &str,
    memory_ctx: &MemoryContext,
    preambles: &Preambles,
) -> RigAgent {
    let system_prompt = format!(
        "{}\n\n## Incident Context\n\n{}\n\n## Analysis\n\n{}",
        PLANNER_PROMPT,
        context.to_prompt_json(),
        analysis_text
    );

    client
        .agent(&config.model)
        .preamble(&system_prompt)
        .append_preamble(&preambles.memory)
        .append_preamble(&preambles.session)
        .max_tokens(config.max_tokens as u64)
        .temperature(f64::from(config.temperature))
        .tool(AskUserTool::new())
        .tool(SavePlanTool)
        .tool(SaveMemoryTool::new(Arc::clone(&memory_ctx.memory_store)))
        .tool(SaveSessionContextTool::new(Arc::clone(
            &memory_ctx.session_context_store,
        )))
        .build()
}

/// Build the `ExecutorAgent`.
///
/// Equipped with `DiagnosticCommandTool` for executing plan commands (HITL on each)
/// and `AskUserTool` for failure-handling decisions. Memory tools are also included.
pub fn build_executor(
    client: &anthropic::Client,
    config: &RigAgentConfig,
    plan_content: &str,
    memory_ctx: &MemoryContext,
    preambles: &Preambles,
) -> RigAgent {
    let system_prompt = format!(
        "{}\n\n## Remediation Plan\n\n{}",
        EXECUTOR_PROMPT, plan_content
    );

    client
        .agent(&config.model)
        .preamble(&system_prompt)
        .append_preamble(&preambles.memory)
        .append_preamble(&preambles.session)
        .max_tokens(config.max_tokens as u64)
        .temperature(f64::from(config.temperature))
        .tool(DiagnosticCommandTool)
        .tool(AskUserTool::new())
        .tool(SaveMemoryTool::new(Arc::clone(&memory_ctx.memory_store)))
        .tool(SaveSessionContextTool::new(Arc::clone(
            &memory_ctx.session_context_store,
        )))
        .build()
}

// ---------------------------------------------------------------------------
// System prompts
// ---------------------------------------------------------------------------

const INVESTIGATOR_PROMPT: &str = "\
You are a senior SRE investigating a production incident.

## Your Mission
Systematically investigate the reported incident by first understanding the \
environment, then collecting diagnostic evidence through structured analysis. \
Your goal is to identify the root cause, not merely confirm the symptom.

## Phase 1: Scoping (MANDATORY — before running any command)

Before executing any diagnostic command, you MUST ask the operator scoping \
questions using `ask_user`. Gather enough context to form an investigation plan.

First, review the incident description provided in the Active Incident section \
below. Extract whatever information is already available — do NOT re-ask what \
the operator already told you.

Key questions to cover (skip those already answered by the incident description):
- What software/services are involved? (web server, reverse proxy, app framework, database)
- What is the infrastructure? (bare metal, VM, containers, cloud provider, orchestrator)
- When did the issue start? Is it intermittent or constant?
- Were there recent changes? (deployments, config changes, infrastructure updates)
- What monitoring/observability is in place? (logs location, metrics, alerting)
- What has already been tried?

Ask questions ONE AT A TIME. Adapt follow-ups based on answers — do not ask \
about Kubernetes if the operator says it is a bare-metal VM.
Ask at minimum 2-3 scoping questions before proceeding to Phase 2. \
Only proceed once you understand the environment well enough to know WHERE to \
look and WHAT to look for.

## Phase 2: Diagnosis

### Mandatory first steps (always run these):
1. Service status — Is the affected service running? Check process status, \
listening ports, service health endpoints.
2. Configuration review — Read the relevant config files (e.g., nginx.conf, \
apache vhosts, app config). Look for misconfigurations, typos, recent edits.

### Guided investigation (pursue based on scoping context):
- Error logs — Check service logs, application logs, system logs (journalctl, \
/var/log/). Look for error patterns, stack traces, timestamps correlating with \
the incident.
- Upstream/backend health — If there is a reverse proxy, check whether backends \
are reachable. Test connectivity, DNS resolution, health check endpoints.
- Resource utilization — CPU, memory, disk, open file descriptors, connection \
counts. Look for exhaustion or anomalies.
- Network and connectivity — Firewall rules, security groups, DNS, TLS \
certificates. Check for blocked ports or expired certs.
- Recent changes on disk — Look for recently modified files in config \
directories, package updates, deployment artifacts.
- Dependency health — Databases, caches, message queues, external APIs that \
the affected service depends on.

Prioritization heuristic: if the operator mentions containers or orchestration, \
prioritize resource utilization and logs. If they mention a reverse proxy, \
prioritize upstream health and configuration. If they mention recent deployments, \
prioritize recent changes on disk and logs.

You do not need to check every dimension — use your judgement based on what you \
learned in Phase 1.

## Proactive Questioning

During diagnosis, use `ask_user` whenever you encounter ambiguity:
- Multiple configuration files or virtual hosts — ask which is relevant.
- Unexpected services or architecture — ask for clarification.
- Findings that suggest multiple possible causes — ask the operator for context.
- Access issues — ask about permissions, credentials, jump hosts.
Do NOT silently guess. When in doubt, ask.

## Tool Usage
- Use `execute_diagnostic_command` for every shell command.
- Set `needs_continuation=true` when you need to process the output to decide \
the next step.
- Set `needs_continuation=false` when the output is self-contained evidence.
- Always specify `motivation`, `risk_level`, and `expected_diagnostic_value`.
- Prefer read-only commands. Only suggest mutations for active remediation \
when evidence clearly points to a fix.

## Risk Levels
- low: read-only (describe, get, list, logs, cat, metrics)
- medium: service restarts, config reads that may affect state
- high: mutations, deletions, scaling operations

## Completion
Stop investigating when you have sufficient evidence to determine:
- The root cause — you can explain WHY the failure is happening, not just WHAT \
is failing.
- The impact scope — what is affected and how.
- A remediation path — what to do about it.
You have sufficient evidence when you have command output that confirms or rules \
out at least 2 candidate root causes.
";

const ANALYST_PROMPT: &str = "\
You are a principal engineer performing root-cause analysis on a production incident.

## Your Mission
Analyse the collected diagnostic evidence and produce a structured analysis.

## Output Format
Respond with a JSON object with EXACTLY these fields:
{
  \"root_cause\": \"<1-2 sentences describing root cause>\",
  \"impact\": \"<services affected, error rates, user impact>\",
  \"affected_services\": [\"service1\", \"service2\"],
  \"timeline\": [
    {\"timestamp\": \"14:00 UTC\", \"description\": \"...\"},
    ...
  ],
  \"fix_plan\": [
    \"Step 1: ...\",
    \"Step 2: ...\",
    ...
  ]
}

Be precise and concise. Base your analysis solely on the collected evidence.
";

const REPORTER_PROMPT: &str = "\
You are a technical writer producing a post-mortem report.

## Your Mission
Write a clear, structured Markdown post-mortem and save it using `save_incident_report`.

## Report Structure
# Post-Mortem: <incident title>

**Date:** YYYY-MM-DD
**Severity:** P0 / P1 / P2
**Status:** Resolved / Ongoing

## Summary
<2-3 sentence executive summary>

## Timeline
| Time (UTC) | Event |
|------------|-------|
| ... | ... |

## Root Cause
<Detailed explanation>

## Impact
<Services, users, duration affected>

## Evidence
<Key findings from diagnostics>

## Fix Plan
1. Immediate: ...
2. Short-term: ...
3. Long-term: ...

## Lessons Learned
- ...

---
*Generated by Infraware incident pipeline*

## Instructions
- Choose a concise slug for the filename (e.g. \"ecs-payments-503\").
- Call `save_incident_report` exactly once with the completed Markdown.
";

const PLANNER_PROMPT: &str = "\
You are a senior SRE creating a remediation plan for a production incident.

## Your Mission
Based on the root-cause analysis, create a detailed, step-by-step remediation plan \
that an operator can follow to fix the issue. The plan must be safe, reversible where \
possible, and include verification steps.

## Plan Format
Write a Markdown document with this structure:

# Remediation Plan: <incident title>

**Date:** YYYY-MM-DD
**Risk Level:** Low / Medium / High (overall)

## Prerequisites
- List any prerequisites (backups, maintenance windows, etc.)

## Steps

### Step 1: <description>
- **Command:** `<exact shell command>`
- **Risk:** Low / Medium / High
- **Expected outcome:** <what should happen>
- **Rollback:** `<command to undo this step>`

### Step 2: ...
(continue for all steps)

## Verification

### Verify 1: <what to verify>
- **Command:** `<verification command>`
- **Expected outcome:** <success criteria>

## Guidelines
- Ask the operator clarifying questions using `ask_user` before finalizing the plan. \
For example, ask about maintenance windows, backup preferences, or which approach \
they prefer when multiple options exist.
- Order steps from least risky to most risky when possible.
- Always include rollback commands for medium and high risk steps.
- The final steps MUST be verification commands that confirm the fix is working.
- Prefer read-only verification (curl, status checks) over mutations.
- Save the plan using `save_remediation_plan` when complete.
";

const EXECUTOR_PROMPT: &str = "\
You are a senior SRE executing a remediation plan for a production incident.

## Your Mission
Execute the remediation plan step by step. For each step, use \
`execute_diagnostic_command` to run the command. Follow the plan order exactly.

## Rules
- Execute ONE step at a time using `execute_diagnostic_command`.
- After each command output, assess whether the step succeeded or failed.
- If a step succeeds, move to the next step.
- If a step fails, use `ask_user` to ask the operator whether to:
  1. Execute the rollback command and retry
  2. Skip this step and continue
  3. Abort the plan execution
- Set `needs_continuation=true` for every command so you can assess the output.
- Set appropriate `risk_level` and `motivation` matching the plan step.
- After executing all steps (including verification), provide a summary of:
  - Steps executed successfully
  - Steps that failed and what action was taken
  - Verification results
  - Overall status (fully resolved / partially resolved / failed)

## Important
- Do NOT skip verification steps.
- Do NOT reorder steps unless a previous step failed and the operator chose to skip.
- Do NOT invent commands not in the plan. Follow the plan exactly.
- When all steps are complete, respond with your summary text (do NOT call any tool).
";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::adapters::rig::incident::context::IncidentContext;

    #[test]
    fn test_save_report_tool_name() {
        assert_eq!(SaveReportTool::NAME, "save_incident_report");
    }

    #[tokio::test]
    async fn test_save_report_tool_definition() {
        let tool = SaveReportTool;
        let def = tool.definition(String::new()).await;
        assert_eq!(def.name, "save_incident_report");
        assert!(def.parameters.is_object());
    }

    #[tokio::test]
    async fn test_save_report_writes_file() {
        use std::fs;

        let tool = SaveReportTool;
        let args = SaveReportArgs {
            slug: "test-incident".to_string(),
            content: "# Test Report\n\nThis is a test.".to_string(),
        };

        let result = tool.call(args).await.unwrap();
        assert!(result.saved);
        assert!(result.path.contains("test-incident"));
        assert!(result.path.ends_with(".md"));

        // Cleanup
        let _ = fs::remove_file(&result.path);
    }

    #[tokio::test]
    async fn test_save_report_rejects_empty_slug() {
        let tool = SaveReportTool;
        let args = SaveReportArgs {
            slug: "../../..".to_string(),
            content: "# Report".to_string(),
        };

        let result = tool.call(args).await;
        assert!(
            result.is_err(),
            "Slug with only path-traversal chars should fail"
        );
    }

    #[tokio::test]
    async fn test_save_report_sanitizes_slug() {
        use std::fs;

        let tool = SaveReportTool;
        let args = SaveReportArgs {
            slug: "../../my-incident/../../hack".to_string(),
            content: "# Report".to_string(),
        };

        let result = tool.call(args).await.unwrap();
        assert!(result.saved);
        assert!(
            !result.path.contains(".."),
            "Path should not contain traversal: {}",
            result.path
        );
        assert!(result.path.contains("my-incidenthack"));

        // Cleanup
        let _ = fs::remove_file(&result.path);
    }

    #[test]
    fn test_system_prompts_not_empty() {
        assert!(!INVESTIGATOR_PROMPT.is_empty());
        assert!(!ANALYST_PROMPT.is_empty());
        assert!(!REPORTER_PROMPT.is_empty());
        assert!(!PLANNER_PROMPT.is_empty());
        assert!(!EXECUTOR_PROMPT.is_empty());
    }

    #[test]
    fn test_investigator_prompt_mentions_key_tools() {
        assert!(INVESTIGATOR_PROMPT.contains("execute_diagnostic_command"));
        assert!(INVESTIGATOR_PROMPT.contains("ask_user"));
    }

    #[test]
    fn test_analyst_prompt_mentions_output_format() {
        assert!(ANALYST_PROMPT.contains("root_cause"));
        assert!(ANALYST_PROMPT.contains("fix_plan"));
    }

    #[test]
    fn test_reporter_prompt_mentions_save_tool() {
        assert!(REPORTER_PROMPT.contains("save_incident_report"));
    }

    #[test]
    fn test_planner_prompt_mentions_key_tools() {
        assert!(PLANNER_PROMPT.contains("ask_user"));
        assert!(PLANNER_PROMPT.contains("save_remediation_plan"));
    }

    #[test]
    fn test_executor_prompt_mentions_key_tools() {
        assert!(EXECUTOR_PROMPT.contains("execute_diagnostic_command"));
        assert!(EXECUTOR_PROMPT.contains("ask_user"));
    }

    #[test]
    fn test_planner_prompt_mentions_verification() {
        assert!(PLANNER_PROMPT.contains("verification"));
        assert!(PLANNER_PROMPT.contains("Verification"));
    }

    #[test]
    fn test_executor_prompt_mentions_rollback() {
        assert!(EXECUTOR_PROMPT.contains("rollback"));
        assert!(EXECUTOR_PROMPT.contains("Abort"));
    }

    #[test]
    fn test_save_plan_tool_name() {
        assert_eq!(SavePlanTool::NAME, "save_remediation_plan");
    }

    #[tokio::test]
    async fn test_save_plan_tool_definition() {
        let tool = SavePlanTool;
        let def = tool.definition(String::new()).await;
        assert_eq!(def.name, "save_remediation_plan");
        assert!(def.parameters.is_object());
    }

    #[tokio::test]
    async fn test_save_plan_writes_file() {
        use std::fs;

        let tool = SavePlanTool;
        let args = SavePlanArgs {
            slug: "test-plan".to_string(),
            content: "# Remediation Plan\n\n1. Fix config".to_string(),
        };

        let result = tool.call(args).await.unwrap();
        assert!(result.saved);
        assert!(result.path.contains("test-plan"));
        assert!(result.path.contains(".infraware/plans/"));
        assert!(result.path.ends_with(".md"));

        // Cleanup
        let _ = fs::remove_file(&result.path);
    }

    #[tokio::test]
    async fn test_save_plan_rejects_empty_slug() {
        let tool = SavePlanTool;
        let args = SavePlanArgs {
            slug: "../../..".to_string(),
            content: "# Plan".to_string(),
        };

        let result = tool.call(args).await;
        assert!(
            result.is_err(),
            "Slug with only path-traversal chars should fail"
        );
    }

    #[tokio::test]
    async fn test_save_plan_sanitizes_slug() {
        use std::fs;

        let tool = SavePlanTool;
        let args = SavePlanArgs {
            slug: "../../my-plan/../../hack".to_string(),
            content: "# Plan".to_string(),
        };

        let result = tool.call(args).await.unwrap();
        assert!(result.saved);
        assert!(
            !result.path.contains(".."),
            "Path should not contain traversal: {}",
            result.path
        );
        assert!(result.path.contains("my-planhack"));

        // Cleanup
        let _ = fs::remove_file(&result.path);
    }
}
