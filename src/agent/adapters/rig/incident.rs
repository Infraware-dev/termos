//! Multi-agent incident investigation pipeline.
//!
//! Sequences three specialised rig-rs agents:
//! 1. `InvestigatorAgent` — collects evidence via CLI (HITL on every command)
//! 2. `AnalystAgent`      — pure LLM reasoning, produces an analysis JSON
//! 3. `ReporterAgent`     — writes the post-mortem Markdown to disk
//!
//! Entry points (called from `orchestrator.rs`):
//! - [`start_investigation`]          — start Phase 1 after operator confirms the incident
//! - [`resume_investigation_command`] — resume Phase 1 after each approved command

pub mod agents;
pub mod context;

use std::sync::Arc;

use async_stream::stream;
use context::{IncidentContext, RiskLevel};
use rig::completion::Prompt;
use rig::providers::anthropic;
use tokio::sync::mpsc;

use super::config::RigAgentConfig;
use super::orchestrator::{HitlHook, InterceptedToolCall};
use super::state::{PendingInterrupt, StateStore};
use super::tools::{DiagnosticCommandArgs, HitlMarker, format_hitl_message};
use crate::agent::error::AgentError;
use crate::agent::shared::{AgentEvent, IncidentPhase, MessageEvent};
use crate::agent::traits::EventStream;

/// Safety guard to avoid endless HITL loops during investigation.
const MAX_INVESTIGATION_COMMANDS: usize = 6;

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Start the incident investigation pipeline (Phase 1: Investigating).
///
/// Called by `create_resume_stream` when the operator approves the
/// `IncidentConfirmation` interrupt.
pub fn start_investigation(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    incident_description: String,
    run_id: String,
) -> EventStream {
    Box::pin(stream! {
        // Emit phase banner
        yield Ok(AgentEvent::phase(IncidentPhase::Investigating));

        let context = IncidentContext::new(incident_description.clone());
        let prompt = format!(
            "You are starting an incident investigation.\n\n\
             Incident: {}\n\n\
             Begin by gathering initial diagnostic evidence. \
             Use execute_diagnostic_command for every shell command you want to run. \
             When you have sufficient evidence, respond with a brief summary of your findings \
             (do NOT call any tool — just write your findings as text).",
            incident_description
        );

        let events = run_investigation_step(
            Arc::clone(&client),
            Arc::clone(&config),
            Arc::clone(&state),
            thread_id.clone(),
            context,
            prompt,
            run_id.clone(),
        );

        for await event in events {
            yield event;
        }
    })
}

/// Resume the investigation after a diagnostic command was approved and executed.
///
/// Called by `create_resume_stream` when the operator approves an
/// `IncidentCommand` interrupt.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields restored from stored interrupt context"
)]
pub fn resume_investigation_command(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    command: String,
    motivation: String,
    needs_continuation: bool,
    risk_level: RiskLevel,
    expected_diagnostic_value: String,
    mut context: IncidentContext,
    run_id: String,
    timeout_secs: u64,
) -> EventStream {
    Box::pin(stream! {
        // Execute the approved diagnostic command
        let raw = super::shell::spawn_command(&command, timeout_secs).await;
        // Normalize empty output — Anthropic API rejects messages with empty content
        let output = if raw.trim().is_empty() {
            "(no output — command produced no stdout/stderr)".to_string()
        } else {
            raw
        };

        // Show command output to operator
        let output_block = format!("```\n$ {}\n{}\n```", command, output.trim());
        yield Ok(AgentEvent::Message(MessageEvent::assistant(&output_block)));

        // Record result in context with the full metadata approved by the operator
        context.add_command_result(context::CommandResult {
            command: command.clone(),
            output: output.clone(),
            motivation,
            risk_level,
            expected_diagnostic_value,
        });

        if !needs_continuation {
            // The command output is self-contained; end the interaction
            yield Ok(AgentEvent::end());
            return;
        }

        // Continue investigation with updated context
        let prompt = format!(
            "You are continuing an incident investigation.\n\n\
             Evidence collected so far:\n{}\n\n\
             The last command you requested was `{}` and here is its output:\n{}\n\n\
             Continue the investigation. If you need more information, call execute_diagnostic_command. \
             If you have sufficient evidence to determine the root cause, respond with your findings \
             as text (do NOT call any tool).",
            context.to_prompt_json(),
            command,
            output.trim()
        );

        let events = run_investigation_step(
            Arc::clone(&client),
            Arc::clone(&config),
            Arc::clone(&state),
            thread_id.clone(),
            context,
            prompt,
            run_id.clone(),
        );

        for await event in events {
            yield event;
        }
    })
}

/// Resume the investigation with a pre-provided command output.
///
/// Called by `create_resume_stream` when the terminal PTY already executed
/// the diagnostic command and sent back a `CommandOutput` resume response.
/// Identical to `resume_investigation_command` but skips the `spawn_command` call.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields restored from stored interrupt context"
)]
pub fn resume_investigation_with_output(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    command: String,
    motivation: String,
    needs_continuation: bool,
    risk_level: RiskLevel,
    expected_diagnostic_value: String,
    mut context: IncidentContext,
    run_id: String,
    output: String,
) -> EventStream {
    Box::pin(stream! {
        // Normalize empty output — Anthropic API rejects messages with empty content
        let output = if output.trim().is_empty() {
            "(no output — command produced no stdout/stderr)".to_string()
        } else {
            output
        };

        // Show command output to operator
        let output_block = format!("```\n$ {}\n{}\n```", command, output.trim());
        yield Ok(AgentEvent::Message(MessageEvent::assistant(&output_block)));

        // Record result in context with the full metadata approved by the operator
        context.add_command_result(context::CommandResult {
            command: command.clone(),
            output: output.clone(),
            motivation,
            risk_level,
            expected_diagnostic_value,
        });

        if !needs_continuation {
            yield Ok(AgentEvent::end());
            return;
        }

        let prompt = format!(
            "You are continuing an incident investigation.\n\n\
             Evidence collected so far:\n{}\n\n\
             The last command you requested was `{}` and here is its output:\n{}\n\n\
             Continue the investigation. If you need more information, call execute_diagnostic_command. \
             If you have sufficient evidence to determine the root cause, respond with your findings \
             as text (do NOT call any tool).",
            context.to_prompt_json(),
            command,
            output.trim()
        );

        let events = run_investigation_step(
            Arc::clone(&client),
            Arc::clone(&config),
            Arc::clone(&state),
            thread_id.clone(),
            context,
            prompt,
            run_id.clone(),
        );

        for await event in events {
            yield event;
        }
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Run a single investigation turn with the InvestigatorAgent.
///
/// If the agent calls `execute_diagnostic_command`, intercepts it and stores
/// an `IncidentCommand` interrupt, then returns.
/// If the agent returns a text response (no tool call), chains to
/// Phase 2 (Analysis) and Phase 3 (Reporting).
fn run_investigation_step(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    context: IncidentContext,
    prompt: String,
    run_id: String,
) -> EventStream {
    Box::pin(stream! {
        let (tx, mut rx) = mpsc::unbounded_channel::<InterceptedToolCall>();
        let hook = HitlHook { tool_call_tx: tx };

        let agent = agents::build_investigator(&client, &config, &context);

        tracing::info!(
            thread_id = %thread_id,
            run_id = %run_id,
            "Running InvestigatorAgent turn"
        );

        let result = agent
            .prompt(&prompt)
            .max_turns(1)
            .with_hook(hook)
            .await;

        // Check if a diagnostic command was intercepted
        if let Ok(intercepted) = rx.try_recv() {
            if intercepted.tool_name == "execute_diagnostic_command"
                && let Ok(args) = serde_json::from_str::<DiagnosticCommandArgs>(&intercepted.args)
            {
                let normalized_cmd = args.command.trim().to_ascii_lowercase();
                let duplicate_count = context
                    .commands_executed
                    .iter()
                    .filter(|r| r.command.trim().to_ascii_lowercase() == normalized_cmd)
                    .count();

                if duplicate_count > 0 {
                    tracing::warn!(
                        run_id = %run_id,
                        command = %args.command,
                        duplicate_count,
                        "Investigator requested a duplicate diagnostic command; forcing analysis phase"
                    );

                    let forced_findings = format!(
                        "Loop guard activated: command `{}` was already executed {} time(s). \
                         Proceeding with available evidence to avoid redundant diagnostics.",
                        args.command, duplicate_count
                    );
                    yield Ok(AgentEvent::Message(MessageEvent::assistant(&forced_findings)));
                    for await event in run_analysis_and_report(
                        Arc::clone(&client),
                        Arc::clone(&config),
                        context,
                        forced_findings,
                        run_id.clone(),
                    ) {
                        yield event;
                    }
                    return;
                }

                if context.commands_executed.len() >= MAX_INVESTIGATION_COMMANDS {
                    tracing::warn!(
                        run_id = %run_id,
                        max_steps = MAX_INVESTIGATION_COMMANDS,
                        "Investigation reached max diagnostic commands; forcing analysis phase"
                    );

                    let forced_findings = format!(
                        "Step limit reached ({} diagnostic commands approved). \
                         Proceeding to analysis with collected evidence.",
                        MAX_INVESTIGATION_COMMANDS
                    );
                    yield Ok(AgentEvent::Message(MessageEvent::assistant(&forced_findings)));
                    for await event in run_analysis_and_report(
                        Arc::clone(&client),
                        Arc::clone(&config),
                        context,
                        forced_findings,
                        run_id.clone(),
                    ) {
                        yield event;
                    }
                    return;
                }

                let hitl_message = format_hitl_message(&args);
                let pending = PendingInterrupt::incident_command(
                    args.command.clone(),
                    args.motivation.clone(),
                    args.needs_continuation,
                    args.risk_level,
                    args.expected_diagnostic_value.clone(),
                    context,
                    intercepted.tool_call_id,
                    serde_json::from_str(&intercepted.args).ok(),
                );
                let _ = state.store_interrupt(&thread_id, pending).await;

                yield Ok(AgentEvent::updates_with_interrupt(
                    HitlMarker::CommandApproval {
                        command: args.command,
                        message: hitl_message,
                        needs_continuation: args.needs_continuation,
                    }.into()
                ));
                return;
            }
            // Unknown tool or parse error — fall through to handle agent text response
            tracing::warn!(tool = %intercepted.tool_name, "Unexpected tool intercepted in investigation");
        }

        // No tool call intercepted — investigation complete
        match result {
            Ok(findings) => {
                tracing::info!(run_id = %run_id, "InvestigatorAgent finished — moving to analysis");

                // Emit findings as assistant message
                if !findings.trim().is_empty() {
                    yield Ok(AgentEvent::Message(MessageEvent::assistant(&findings)));
                }

                // Phase 2: Analysis
                for await event in run_analysis_and_report(
                    Arc::clone(&client),
                    Arc::clone(&config),
                    context,
                    findings,
                    run_id.clone(),
                ) {
                    yield event;
                }
            }
            Err(e) => {
                tracing::error!(run_id = %run_id, error = ?e, "InvestigatorAgent failed");
                yield Err(AgentError::Other(anyhow::anyhow!("Investigator error: {}", e)));
            }
        }
    })
}

/// Run Phase 2 (AnalystAgent) followed by Phase 3 (ReporterAgent).
fn run_analysis_and_report(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    context: IncidentContext,
    investigation_findings: String,
    run_id: String,
) -> EventStream {
    Box::pin(stream! {
        // Phase 2 — Analysis
        yield Ok(AgentEvent::phase(IncidentPhase::Analyzing));

        let analyst_prompt = format!(
            "Analyse the following incident evidence and provide a structured analysis.\n\n\
             Evidence:\n{}\n\n\
             Investigator findings:\n{}",
            context.to_prompt_json(),
            investigation_findings
        );

        let analyst = agents::build_analyst(&client, &config, &context);
        let analysis_result = analyst.prompt(&analyst_prompt).await;

        let analysis_text = match analysis_result {
            Ok(text) => text,
            Err(e) => {
                tracing::error!(run_id = %run_id, error = ?e, "AnalystAgent failed");
                yield Err(AgentError::Other(anyhow::anyhow!("Analyst error: {}", e)));
                return;
            }
        };

        tracing::info!(run_id = %run_id, "AnalystAgent finished analysis");

        // Phase 3 — Reporting
        yield Ok(AgentEvent::phase(IncidentPhase::Reporting));

        let (tx, mut rx) = mpsc::unbounded_channel::<InterceptedToolCall>();
        let hook = HitlHook { tool_call_tx: tx };

        let reporter = agents::build_reporter(&client, &config, &context, &analysis_text);
        let reporter_prompt = "Write the post-mortem report and save it using save_incident_report.";

        let report_result = reporter
            .prompt(reporter_prompt)
            .max_turns(3)
            .with_hook(hook)
            .await;

        // The reporter uses SaveReportTool which is NOT intercepted (no HITL for file writes)
        // If any tool was intercepted, it's unexpected
        if let Ok(intercepted) = rx.try_recv() {
            tracing::warn!(tool = %intercepted.tool_name, "Unexpected tool intercepted in reporter");
        }

        match report_result {
            Ok(response) => {
                tracing::info!(run_id = %run_id, "ReporterAgent finished");
                if !response.trim().is_empty() {
                    yield Ok(AgentEvent::Message(MessageEvent::assistant(&response)));
                }
                yield Ok(AgentEvent::phase(IncidentPhase::Completed));
                yield Ok(AgentEvent::end());
            }
            Err(e) => {
                tracing::error!(run_id = %run_id, error = ?e, "ReporterAgent failed");
                yield Err(AgentError::Other(anyhow::anyhow!("Reporter error: {}", e)));
            }
        }
    })
}
