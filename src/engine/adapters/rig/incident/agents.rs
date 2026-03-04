//! Builder functions for the three incident-investigation rig agents.
//!
//! Each function returns a configured `RigAgent` bound to the Anthropic client:
//!
//! - `build_investigator` — HITL command collection (DiagnosticCommandTool + AskUserTool)
//! - `build_analyst`      — pure LLM root-cause analysis (no tools)
//! - `build_reporter`     — post-mortem Markdown writer (SaveReportTool)

use rig::client::CompletionClient as _;
use rig::completion::ToolDefinition;
use rig::providers::anthropic;
use rig::tool::Tool;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};

use super::context::IncidentContext;
use crate::engine::adapters::rig::config::RigEngineConfig;
use crate::engine::adapters::rig::orchestrator::RigAgent;
use crate::engine::adapters::rig::tools::{AskUserTool, DiagnosticCommandTool};

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
// Agent builder functions
// ---------------------------------------------------------------------------

/// Build the `InvestigatorAgent`.
///
/// Equipped with `DiagnosticCommandTool` (HITL on every command) and
/// `AskUserTool` for clarifying questions.
pub fn build_investigator(
    client: &anthropic::Client,
    config: &RigEngineConfig,
    context: &IncidentContext,
) -> RigAgent {
    let system_prompt = format!(
        "{}\n\n## Active Incident\n\n{}",
        INVESTIGATOR_PROMPT,
        context.to_prompt_json()
    );

    client
        .agent(&config.model)
        .preamble(&system_prompt)
        .max_tokens(config.max_tokens as u64)
        .temperature(f64::from(config.temperature))
        .tool(DiagnosticCommandTool)
        .tool(AskUserTool::new())
        .build()
}

/// Build the `AnalystAgent`.
///
/// No tools — pure LLM reasoning over the collected evidence.
/// Produces a structured `AnalysisReport` JSON in its response.
pub fn build_analyst(
    client: &anthropic::Client,
    config: &RigEngineConfig,
    context: &IncidentContext,
) -> RigAgent {
    let system_prompt = format!(
        "{}\n\n## Evidence Collected\n\n{}",
        ANALYST_PROMPT,
        context.to_prompt_json()
    );

    client
        .agent(&config.model)
        .preamble(&system_prompt)
        .max_tokens(config.max_tokens as u64)
        .temperature(f64::from(config.temperature))
        .build()
}

/// Build the `ReporterAgent`.
///
/// Equipped with `SaveReportTool` to persist the Markdown post-mortem.
pub fn build_reporter(
    client: &anthropic::Client,
    config: &RigEngineConfig,
    context: &IncidentContext,
    analysis_json: &str,
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
        .max_tokens(config.max_tokens as u64)
        .temperature(f64::from(config.temperature))
        .tool(SaveReportTool)
        .build()
}

// ---------------------------------------------------------------------------
// System prompts
// ---------------------------------------------------------------------------

const INVESTIGATOR_PROMPT: &str = "\
You are a senior SRE investigating a production incident.

## Your Mission
Collect diagnostic evidence from cloud CLI tools (aws, gcloud, az, kubectl, docker).
Run READ-ONLY commands unless mutation is strictly necessary for remediation.

## Guidelines
- Use `execute_diagnostic_command` for every shell command.
- Set `needs_continuation=true` when you need to process the output to decide the next step.
- Set `needs_continuation=false` when the output is self-contained evidence.
- Always specify `motivation`, `risk_level`, and `expected_diagnostic_value`.
- Use `ask_user` if you need information only the operator can provide.
- Stop when you have enough evidence to determine root cause, impact, and remediation.

## Risk Levels
- low: read-only (describe, get, list, logs, metrics)
- medium: service restarts, config reads that may affect state
- high: mutations, deletions, scaling operations
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::adapters::rig::incident::context::IncidentContext;

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
}
