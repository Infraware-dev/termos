//! Tool that lets `NormalAgent` escalate to the incident investigation pipeline.
//!
//! When `NormalAgent` detects an incident investigation intent, it calls
//! `start_incident_investigation`.  The tool returns `PendingConfirmation`
//! which the orchestrator converts into a HITL "y/n" prompt.  On operator
//! approval, the orchestrator hands off to `IncidentOrchestrator`.

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};

/// Arguments for the incident investigation escalation tool.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct StartIncidentArgs {
    /// A concise description of the incident (who, what, since when)
    #[schemars(
        description = "Describe the incident: affected service, observed symptoms, and approximate start time (e.g., 'ECS service payments is returning 503 since 14:00 UTC')"
    )]
    pub incident_description: String,
}

/// Result returned by `StartIncidentInvestigationTool::call()`.
///
/// Always `PendingConfirmation` — the orchestrator turns this into an
/// operator confirmation HITL prompt before launching the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StartIncidentResult {
    /// Awaiting operator confirmation to start the multi-agent pipeline
    PendingConfirmation { incident_description: String },
}

/// Error type for the incident escalation tool.
#[derive(Debug, thiserror::Error)]
pub enum StartIncidentError {
    #[error("Incident description must not be empty")]
    EmptyDescription,
}

/// Tool registered on `NormalAgent` to escalate to the incident pipeline.
///
/// `call()` always returns `PendingConfirmation`; no side effects occur
/// until the operator approves the HITL prompt.
#[derive(Debug, Clone, Default)]
pub struct StartIncidentInvestigationTool;

impl Tool for StartIncidentInvestigationTool {
    const NAME: &'static str = "start_incident_investigation";

    type Error = StartIncidentError;
    type Args = StartIncidentArgs;
    type Output = StartIncidentResult;

    #[expect(
        clippy::manual_async_fn,
        reason = "rig-rs Tool trait requires impl Future return type"
    )]
    fn definition(&self, _prompt: String) -> impl Future<Output = ToolDefinition> + Send + Sync {
        async {
            ToolDefinition {
                name: Self::NAME.to_string(),
                description: "Launch a multi-agent incident investigation pipeline. \
                    Use this tool when the user asks you to investigate an incident, outage, \
                    service crash, degradation, or perform a post-mortem analysis. \
                    Provide a concise description including the affected service and \
                    approximate start time. The operator will confirm before the pipeline starts."
                    .to_string(),
                parameters: serde_json::to_value(schema_for!(StartIncidentArgs))
                    .expect("Failed to generate JSON schema for StartIncidentArgs"),
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
            if args.incident_description.trim().is_empty() {
                return Err(StartIncidentError::EmptyDescription);
            }
            Ok(StartIncidentResult::PendingConfirmation {
                incident_description: args.incident_description,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        assert_eq!(
            StartIncidentInvestigationTool::NAME,
            "start_incident_investigation"
        );
    }

    #[tokio::test]
    async fn test_call_returns_pending_confirmation() {
        let tool = StartIncidentInvestigationTool;
        let args = StartIncidentArgs {
            incident_description: "ECS service payments returning 503 since 14:00 UTC".to_string(),
        };

        let result = tool.call(args).await.unwrap();

        match result {
            StartIncidentResult::PendingConfirmation {
                incident_description,
            } => {
                assert!(incident_description.contains("payments"));
            }
        }
    }

    #[tokio::test]
    async fn test_call_rejects_empty_description() {
        let tool = StartIncidentInvestigationTool;
        let args = StartIncidentArgs {
            incident_description: "   ".to_string(),
        };

        let result = tool.call(args).await;
        assert!(matches!(result, Err(StartIncidentError::EmptyDescription)));
    }

    #[tokio::test]
    async fn test_tool_definition() {
        let tool = StartIncidentInvestigationTool;
        let def = tool.definition(String::new()).await;
        assert_eq!(def.name, "start_incident_investigation");
        assert!(def.description.contains("incident"));
        assert!(def.parameters.is_object());
    }

    #[test]
    fn test_args_schema_contains_description_field() {
        let schema = schema_for!(StartIncidentArgs);
        let json = serde_json::to_string(&schema).unwrap();
        assert!(json.contains("incident_description"));
    }
}
