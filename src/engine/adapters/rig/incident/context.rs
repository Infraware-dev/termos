//! Data models for the incident investigation pipeline.
//!
//! `IncidentContext` accumulates evidence collected by `InvestigatorAgent`
//! and is passed to `AnalystAgent` and `ReporterAgent` in sequence.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Risk level associated with a diagnostic command.
///
/// Shown to the operator at HITL approval time so they can make an
/// informed decision before execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "Low"),
            RiskLevel::Medium => write!(f, "Medium"),
            RiskLevel::High => write!(f, "High"),
        }
    }
}

/// Significance of a finding relative to the root cause investigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSignificance {
    Low,
    Medium,
    High,
}

/// Raw result of a diagnostic command executed during investigation.
///
/// Recorded in `IncidentContext.commands_executed` for every approved
/// command, regardless of its significance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    /// The shell command that was executed
    pub command: String,
    /// Combined stdout/stderr output
    pub output: String,
    /// Why this command was needed (from `DiagnosticCommandTool`)
    pub motivation: String,
    /// Risk level declared by the agent at call time
    pub risk_level: RiskLevel,
    /// What diagnostic value was expected
    pub expected_diagnostic_value: String,
}

/// A meaningful finding extracted from command output.
///
/// `InvestigatorAgent` promotes a `CommandResult` to a `Finding` when the
/// output contains evidence relevant to the root cause.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// The command that produced this finding
    pub command: String,
    /// Relevant portion of the output
    pub output: String,
    /// How significant this finding is for the root cause
    pub significance: FindingSignificance,
    /// Motivation that was declared for the command
    pub motivation: String,
    /// Risk level of the command
    pub risk_level: RiskLevel,
    /// Expected diagnostic value declared at call time
    pub expected_diagnostic_value: String,
}

/// A single event in the incident timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    /// When the event occurred (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
    /// Human-readable description of the event
    pub description: String,
}

/// Structured analysis produced by `AnalystAgent`.
///
/// Contains the RCA output required by acceptance criteria:
/// Timeline, Root Cause, and Fix Plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisReport {
    /// Root cause of the incident
    pub root_cause: String,
    /// Business / operational impact
    pub impact: String,
    /// Services affected by the incident
    pub affected_services: Vec<String>,
    /// Ordered timeline of events leading to the incident
    pub timeline: Vec<TimelineEvent>,
    /// Ordered remediation steps (the "Fix Plan")
    pub fix_plan: Vec<String>,
}

/// Shared context passed between the three incident pipeline agents.
///
/// `InvestigatorAgent` populates it; `AnalystAgent` and `ReporterAgent`
/// consume it read-only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentContext {
    /// Human-provided description of the incident
    pub description: String,
    /// When the investigation started
    pub started_at: DateTime<Utc>,
    /// All commands executed during investigation (in order)
    pub commands_executed: Vec<CommandResult>,
    /// Significant findings promoted from command results
    pub findings: Vec<Finding>,
}

impl IncidentContext {
    /// Create a new context for the given incident description.
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            started_at: Utc::now(),
            commands_executed: Vec::new(),
            findings: Vec::new(),
        }
    }

    /// Record a command result after HITL approval and execution.
    pub fn add_command_result(&mut self, result: CommandResult) {
        self.commands_executed.push(result);
    }

    /// Record a significant finding.
    #[expect(
        dead_code,
        reason = "Available for InvestigatorAgent to promote command results to findings"
    )]
    pub fn add_finding(&mut self, finding: Finding) {
        self.findings.push(finding);
    }

    /// Serialize the context to pretty JSON for injection into agent prompts.
    pub fn to_prompt_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incident_context_new() {
        let ctx = IncidentContext::new("ECS service down");
        assert_eq!(ctx.description, "ECS service down");
        assert!(ctx.commands_executed.is_empty());
        assert!(ctx.findings.is_empty());
    }

    #[test]
    fn test_add_command_result() {
        let mut ctx = IncidentContext::new("test incident");
        ctx.add_command_result(CommandResult {
            command: "aws ecs describe-services".to_string(),
            output: "INACTIVE".to_string(),
            motivation: "Check service status".to_string(),
            risk_level: RiskLevel::Low,
            expected_diagnostic_value: "Service health information".to_string(),
        });
        assert_eq!(ctx.commands_executed.len(), 1);
    }

    #[test]
    fn test_add_finding() {
        let mut ctx = IncidentContext::new("test incident");
        ctx.add_finding(Finding {
            command: "aws ecs describe-services".to_string(),
            output: "INACTIVE".to_string(),
            significance: FindingSignificance::High,
            motivation: "Check service status".to_string(),
            risk_level: RiskLevel::Low,
            expected_diagnostic_value: "Service health".to_string(),
        });
        assert_eq!(ctx.findings.len(), 1);
        assert_eq!(
            ctx.findings
                .first()
                .expect("finding was added")
                .significance,
            FindingSignificance::High
        );
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut ctx = IncidentContext::new("ECS crash");
        ctx.add_command_result(CommandResult {
            command: "kubectl get pods".to_string(),
            output: "CrashLoopBackOff".to_string(),
            motivation: "Check pod status".to_string(),
            risk_level: RiskLevel::Low,
            expected_diagnostic_value: "Pod health".to_string(),
        });

        let json = serde_json::to_string(&ctx).unwrap();
        let restored: IncidentContext = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.description, "ECS crash");
        assert_eq!(restored.commands_executed.len(), 1);
        assert_eq!(restored.commands_executed[0].risk_level, RiskLevel::Low);
    }

    #[test]
    fn test_risk_level_display() {
        assert_eq!(RiskLevel::Low.to_string(), "Low");
        assert_eq!(RiskLevel::Medium.to_string(), "Medium");
        assert_eq!(RiskLevel::High.to_string(), "High");
    }

    #[test]
    fn test_analysis_report_serde() {
        let report = AnalysisReport {
            root_cause: "OOM kill".to_string(),
            impact: "Service unavailable for 10 min".to_string(),
            affected_services: vec!["payments".to_string()],
            timeline: vec![TimelineEvent {
                timestamp: None,
                description: "Memory spike at 14:02".to_string(),
            }],
            fix_plan: vec!["Increase memory limit to 2Gi".to_string()],
        };
        let json = serde_json::to_string(&report).unwrap();
        let restored: AnalysisReport = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.root_cause, "OOM kill");
        assert_eq!(restored.fix_plan.len(), 1);
    }

    #[test]
    fn test_to_prompt_json_is_valid() {
        let ctx = IncidentContext::new("test");
        let json = ctx.to_prompt_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("description").is_some());
        assert!(parsed.get("commands_executed").is_some());
    }
}
