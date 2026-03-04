//! Diagnostic command tool for the InvestigatorAgent.
//!
//! `DiagnosticCommandTool` is a narrowly-scoped tool used **only** by
//! `InvestigatorAgent`.  It differs from `ShellCommandTool` by requiring
//! three additional fields that satisfy the operational acceptance criteria:
//!
//! - `motivation`                — why this command is needed
//! - `risk_level`                — `low | medium | high`
//! - `expected_diagnostic_value` — what finding we expect from the output
//!
//! These fields are shown to the operator at HITL approval time and stored
//! in `IncidentContext` for the post-mortem report.

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};

use crate::agent::adapters::rig::incident::context::RiskLevel;

/// Arguments for the diagnostic command tool.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct DiagnosticCommandArgs {
    /// The shell command to execute
    #[schemars(description = "The diagnostic shell command to execute (read-only preferred)")]
    pub command: String,

    /// Why this specific command is needed at this point in the investigation
    #[schemars(
        description = "Explain why this command is needed for the investigation (e.g., 'Verify if the ECS service is in a stopped state')"
    )]
    pub motivation: String,

    /// Risk level of executing this command on the target system
    #[schemars(
        description = "Risk level: low (read-only), medium (restarts/config reads), high (mutations/deletes)"
    )]
    pub risk_level: RiskLevel,

    /// What diagnostic value we expect to extract from the output
    #[schemars(
        description = "Describe what finding you expect (e.g., 'CPU utilisation above 90% indicating resource exhaustion')"
    )]
    pub expected_diagnostic_value: String,

    /// Whether the agent needs to process the output to continue investigating.
    /// Set to `true` for intermediate information-gathering commands.
    /// Set to `false` when the output directly closes the investigation step.
    #[schemars(
        description = "true = agent needs to process output to continue; false = output is self-contained"
    )]
    #[serde(default)]
    pub needs_continuation: bool,
}

/// Result type returned by `DiagnosticCommandTool::call()`.
///
/// Always `PendingApproval` — the orchestrator intercepts this result and
/// converts it into a HITL interrupt for operator approval before execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DiagnosticCommandResult {
    /// Command intercepted; awaiting operator HITL approval
    PendingApproval {
        command: String,
        motivation: String,
        risk_level: RiskLevel,
        expected_diagnostic_value: String,
        needs_continuation: bool,
    },
}

/// Error type for the diagnostic command tool.
#[derive(Debug, thiserror::Error)]
pub enum DiagnosticCommandError {
    /// Serialisation of the result failed (programming error)
    #[error("Failed to serialise diagnostic command result: {0}")]
    Serialisation(String),
}

/// Tool used exclusively by `InvestigatorAgent` to execute diagnostic commands
/// with mandatory risk metadata.
///
/// `call()` always returns `PendingApproval`; the actual execution and
/// context recording happen in the orchestrator after operator approval.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticCommandTool;

impl Tool for DiagnosticCommandTool {
    const NAME: &'static str = "execute_diagnostic_command";

    type Error = DiagnosticCommandError;
    type Args = DiagnosticCommandArgs;
    type Output = DiagnosticCommandResult;

    #[expect(
        clippy::manual_async_fn,
        reason = "rig-rs Tool trait requires impl Future return type"
    )]
    fn definition(&self, _prompt: String) -> impl Future<Output = ToolDefinition> + Send + Sync {
        async {
            ToolDefinition {
                name: Self::NAME.to_string(),
                description: "Execute a diagnostic command during incident investigation. \
                    Use this tool to collect evidence from cloud CLI tools (aws, gcloud, az, kubectl). \
                    Prefer read-only commands. Always provide a clear motivation, risk level, \
                    and expected diagnostic value. The operator will approve each command \
                    before it runs."
                    .to_string(),
                parameters: serde_json::to_value(schema_for!(DiagnosticCommandArgs))
                    .expect("Failed to generate JSON schema for DiagnosticCommandArgs"),
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
            Ok(DiagnosticCommandResult::PendingApproval {
                command: args.command,
                motivation: args.motivation,
                risk_level: args.risk_level,
                expected_diagnostic_value: args.expected_diagnostic_value,
                needs_continuation: args.needs_continuation,
            })
        }
    }
}

/// Format the HITL approval message shown to the operator.
///
/// Encodes the rich diagnostic metadata into the human-readable `message`
/// field so the existing terminal renderer can display it without changes.
pub fn format_hitl_message(args: &DiagnosticCommandArgs) -> String {
    format!(
        "Motivation: {}\nRisk: {} | Expected: {}",
        args.motivation, args.risk_level, args.expected_diagnostic_value
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        assert_eq!(DiagnosticCommandTool::NAME, "execute_diagnostic_command");
    }

    #[tokio::test]
    async fn test_call_returns_pending_approval() {
        let tool = DiagnosticCommandTool;
        let args = DiagnosticCommandArgs {
            command: "aws ecs describe-services --cluster prod".to_string(),
            motivation: "Check service health".to_string(),
            risk_level: RiskLevel::Low,
            expected_diagnostic_value: "Service status (ACTIVE/INACTIVE)".to_string(),
            needs_continuation: true,
        };

        let result = tool.call(args).await.unwrap();

        match result {
            DiagnosticCommandResult::PendingApproval {
                command,
                motivation,
                risk_level,
                needs_continuation,
                ..
            } => {
                assert_eq!(command, "aws ecs describe-services --cluster prod");
                assert_eq!(motivation, "Check service health");
                assert_eq!(risk_level, RiskLevel::Low);
                assert!(needs_continuation);
            }
        }
    }

    #[tokio::test]
    async fn test_call_high_risk_command() {
        let tool = DiagnosticCommandTool;
        let args = DiagnosticCommandArgs {
            command: "kubectl delete pod payments-77b9f -n prod".to_string(),
            motivation: "Force pod restart to clear OOM state".to_string(),
            risk_level: RiskLevel::High,
            expected_diagnostic_value: "Pod will restart and clear memory pressure".to_string(),
            needs_continuation: false,
        };

        let result = tool.call(args).await.unwrap();

        match result {
            DiagnosticCommandResult::PendingApproval { risk_level, .. } => {
                assert_eq!(risk_level, RiskLevel::High);
            }
        }
    }

    #[test]
    fn test_format_hitl_message() {
        let args = DiagnosticCommandArgs {
            command: "aws cloudwatch get-metric-statistics".to_string(),
            motivation: "Check CPU utilisation".to_string(),
            risk_level: RiskLevel::Low,
            expected_diagnostic_value: "CPU spike above 90%".to_string(),
            needs_continuation: true,
        };
        let msg = format_hitl_message(&args);
        assert!(msg.contains("CPU utilisation"));
        assert!(msg.contains("Low"));
        assert!(msg.contains("CPU spike above 90%"));
    }

    #[tokio::test]
    async fn test_tool_definition() {
        let tool = DiagnosticCommandTool;
        let def = tool.definition(String::new()).await;
        assert_eq!(def.name, "execute_diagnostic_command");
        assert!(def.description.contains("diagnostic"));
        assert!(def.parameters.is_object());
    }

    #[test]
    fn test_args_schema_contains_required_fields() {
        let schema = schema_for!(DiagnosticCommandArgs);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("command"));
        assert!(json.contains("motivation"));
        assert!(json.contains("risk_level"));
        assert!(json.contains("expected_diagnostic_value"));
    }
}
