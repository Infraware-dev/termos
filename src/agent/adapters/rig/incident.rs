//! Multi-agent incident investigation pipeline.
//!
//! Sequences five specialised rig-rs agents:
//! 1. `InvestigatorAgent` — collects evidence via CLI (HITL on every command)
//! 2. `AnalystAgent`      — pure LLM reasoning, produces an analysis JSON
//! 3. `ReporterAgent`     — writes the post-mortem Markdown to disk
//! 4. `PlannerAgent`      — creates a remediation plan (HITL for scoping questions)
//! 5. `ExecutorAgent`     — executes the plan step by step (HITL on every command)
//!
//! Entry points (called from `orchestrator.rs`):
//! - [`start_investigation`]          — start Phase 1 after operator confirms the incident
//! - [`resume_investigation_command`] — resume Phase 1 after each approved command
//! - [`start_planning`]               — start Phase 4 after operator confirms plan creation
//! - [`resume_planning_question`]     — resume Phase 4 after operator answers planner question
//! - [`start_plan_review`]            — show plan and ask for changes
//! - [`start_execution`]              — start Phase 5 after operator confirms execution
//! - [`resume_execution_command`]     — resume Phase 5 after each approved command
//! - [`resume_execution_with_output`] — resume Phase 5 with PTY-captured output
//! - [`resume_execution_question`]    — resume Phase 5 after operator answers executor question

pub mod agents;
pub mod context;

use std::sync::Arc;

use async_stream::stream;
use context::{IncidentContext, RiskLevel};
use rig::completion::Prompt;
use rig::providers::anthropic;
use tokio::sync::mpsc;

use super::config::RigAgentConfig;
use super::memory::MemoryContext;
use super::orchestrator::{HitlHook, InterceptedToolCall};
use super::state::{PendingInterrupt, StateStore};
use super::tools::{AskUserArgs, DiagnosticCommandArgs, HitlMarker, format_hitl_message};
use crate::agent::error::AgentError;
use crate::agent::shared::{AgentEvent, IncidentPhase, MessageEvent};
use crate::agent::traits::EventStream;

/// Safety guard to avoid endless HITL loops during investigation.
const MAX_INVESTIGATION_COMMANDS: usize = 50;

/// Safety guard to avoid endless plan revision loops.
const MAX_PLAN_REVISIONS: usize = 10;

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
    memory_ctx: MemoryContext,
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
            memory_ctx.clone(),
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
    memory_ctx: MemoryContext,
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
            memory_ctx.clone(),
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
    memory_ctx: MemoryContext,
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
            memory_ctx.clone(),
        );

        for await event in events {
            yield event;
        }
    })
}

/// Resume the investigation after the operator answered a scoping/clarification question.
///
/// Called by `create_resume_stream` when the operator answers an
/// `IncidentQuestion` interrupt.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields restored from stored interrupt context"
)]
pub fn resume_investigation_question(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    question: String,
    answer: String,
    mut context: IncidentContext,
    run_id: String,
    memory_ctx: MemoryContext,
) -> EventStream {
    Box::pin(stream! {
        // Record the Q&A exchange so subsequent turns retain it
        context.add_question_answer(&question, &answer);

        let prompt = format!(
            "You are continuing an incident investigation.\n\n\
             Evidence collected so far:\n{}\n\n\
             You asked the operator: \"{}\"\n\
             The operator answered: \"{}\"\n\n\
             Continue the investigation. Use this information to guide your next steps. \
             If you need more information from the operator, use ask_user. \
             If you need to run a diagnostic command, use execute_diagnostic_command. \
             If you have sufficient evidence to determine the root cause, respond with \
             your findings as text (do NOT call any tool).",
            context.to_prompt_json(),
            question,
            answer
        );

        let events = run_investigation_step(
            Arc::clone(&client),
            Arc::clone(&config),
            Arc::clone(&state),
            thread_id.clone(),
            context,
            prompt,
            run_id.clone(),
            memory_ctx.clone(),
        );

        for await event in events {
            yield event;
        }
    })
}

/// Start the planning phase (Phase 4: Planning).
///
/// Called by `create_resume_stream` when the operator confirms plan creation
/// at the `IncidentPlanConfirmation` gate.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields required to drive planner agent + state + memory"
)]
pub fn start_planning(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    context: IncidentContext,
    analysis_text: String,
    run_id: String,
    memory_ctx: MemoryContext,
) -> EventStream {
    Box::pin(stream! {
        yield Ok(AgentEvent::phase(IncidentPhase::Planning));

        let prompt = "Create a detailed remediation plan based on the incident analysis. \
                      Start by asking the operator any clarifying questions you need, \
                      then write and save the plan.";

        let events = run_planning_step(
            Arc::clone(&client),
            Arc::clone(&config),
            Arc::clone(&state),
            thread_id.clone(),
            context,
            analysis_text,
            prompt.to_string(),
            0, // revision_round
            run_id.clone(),
            memory_ctx.clone(),
        );

        for await event in events {
            yield event;
        }
    })
}

/// Resume planning after the operator answered a planner question.
///
/// Called by `create_resume_stream` when the operator answers an
/// `IncidentPlannerQuestion` interrupt.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields restored from stored interrupt context"
)]
pub fn resume_planning_question(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    question: String,
    answer: String,
    context: IncidentContext,
    analysis_text: String,
    revision_round: usize,
    run_id: String,
    memory_ctx: MemoryContext,
) -> EventStream {
    Box::pin(stream! {
        // Record the planning Q&A into the context so subsequent agent
        // rebuilds see the full history in their system prompt.
        let mut context = context;
        context.add_question_answer(&question, &answer);

        let prompt = format!(
            "You asked the operator: \"{}\"\n\
             The operator answered: \"{}\"\n\n\
             Continue creating the remediation plan based on this information. \
             If you need more clarification, use ask_user. \
             When the plan is ready, save it using save_remediation_plan.",
            question, answer
        );

        let events = run_planning_step(
            Arc::clone(&client),
            Arc::clone(&config),
            Arc::clone(&state),
            thread_id.clone(),
            context,
            analysis_text,
            prompt,
            revision_round,
            run_id.clone(),
            memory_ctx.clone(),
        );

        for await event in events {
            yield event;
        }
    })
}

/// Start the execution phase (Phase 5: Executing).
///
/// Called by `create_resume_stream` when the operator confirms plan execution
/// at the `IncidentExecutionConfirmation` gate.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields required to drive executor agent + state + memory"
)]
pub fn start_execution(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    plan_content: String,
    plan_path: String,
    run_id: String,
    memory_ctx: MemoryContext,
) -> EventStream {
    Box::pin(stream! {
        yield Ok(AgentEvent::phase(IncidentPhase::Executing));

        let prompt = "Execute the remediation plan step by step. Start with step 1.";

        let events = run_execution_step(
            Arc::clone(&client),
            Arc::clone(&config),
            Arc::clone(&state),
            thread_id.clone(),
            plan_content,
            plan_path,
            prompt.to_string(),
            run_id.clone(),
            memory_ctx.clone(),
        );

        for await event in events {
            yield event;
        }
    })
}

/// Resume execution after a remediation command was approved and executed.
///
/// Called by `create_resume_stream` when the operator approves an
/// `IncidentPlanCommand` interrupt.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields restored from stored interrupt context"
)]
pub fn resume_execution_command(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    command: String,
    motivation: String,
    needs_continuation: bool,
    plan_content: String,
    plan_path: String,
    run_id: String,
    timeout_secs: u64,
    memory_ctx: MemoryContext,
) -> EventStream {
    Box::pin(stream! {
        let raw = super::shell::spawn_command(&command, timeout_secs).await;
        let output = if raw.trim().is_empty() {
            "(no output — command produced no stdout/stderr)".to_string()
        } else {
            raw
        };

        let output_block = format!("```\n$ {}\n{}\n```", command, output.trim());
        yield Ok(AgentEvent::Message(MessageEvent::assistant(&output_block)));

        if !needs_continuation {
            yield Ok(AgentEvent::end());
            return;
        }

        let prompt = format!(
            "You are executing a remediation plan.\n\n\
             The command `{}` (motivation: {}) produced this output:\n{}\n\n\
             Assess whether this step succeeded. If it did, continue to the next step. \
             If it failed, use ask_user to ask the operator what to do.",
            command, motivation, output.trim()
        );

        let events = run_execution_step(
            Arc::clone(&client),
            Arc::clone(&config),
            Arc::clone(&state),
            thread_id.clone(),
            plan_content,
            plan_path,
            prompt,
            run_id.clone(),
            memory_ctx.clone(),
        );

        for await event in events {
            yield event;
        }
    })
}

/// Resume execution with pre-provided command output from the terminal PTY.
///
/// Identical to `resume_execution_command` but skips `spawn_command`.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields restored from stored interrupt context"
)]
pub fn resume_execution_with_output(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    command: String,
    motivation: String,
    needs_continuation: bool,
    plan_content: String,
    plan_path: String,
    run_id: String,
    output: String,
    memory_ctx: MemoryContext,
) -> EventStream {
    Box::pin(stream! {
        let output = if output.trim().is_empty() {
            "(no output — command produced no stdout/stderr)".to_string()
        } else {
            output
        };

        let output_block = format!("```\n$ {}\n{}\n```", command, output.trim());
        yield Ok(AgentEvent::Message(MessageEvent::assistant(&output_block)));

        if !needs_continuation {
            yield Ok(AgentEvent::end());
            return;
        }

        let prompt = format!(
            "You are executing a remediation plan.\n\n\
             The command `{}` (motivation: {}) produced this output:\n{}\n\n\
             Assess whether this step succeeded. If it did, continue to the next step. \
             If it failed, use ask_user to ask the operator what to do.",
            command, motivation, output.trim()
        );

        let events = run_execution_step(
            Arc::clone(&client),
            Arc::clone(&config),
            Arc::clone(&state),
            thread_id.clone(),
            plan_content,
            plan_path,
            prompt,
            run_id.clone(),
            memory_ctx.clone(),
        );

        for await event in events {
            yield event;
        }
    })
}

/// Resume execution after the operator answered a question (e.g., rollback/skip/abort).
///
/// Called by `create_resume_stream` when the operator answers an
/// `IncidentExecutorQuestion` interrupt.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields restored from stored interrupt context"
)]
pub fn resume_execution_question(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    question: String,
    answer: String,
    plan_content: String,
    plan_path: String,
    run_id: String,
    memory_ctx: MemoryContext,
) -> EventStream {
    Box::pin(stream! {
        let prompt = format!(
            "You are executing a remediation plan.\n\n\
             You asked the operator: \"{}\"\n\
             The operator answered: \"{}\"\n\n\
             Continue executing the plan based on this response.",
            question, answer
        );

        let events = run_execution_step(
            Arc::clone(&client),
            Arc::clone(&config),
            Arc::clone(&state),
            thread_id.clone(),
            plan_content,
            plan_path,
            prompt,
            run_id.clone(),
            memory_ctx.clone(),
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
#[expect(
    clippy::too_many_arguments,
    reason = "All fields required to drive investigator agent + state + memory"
)]
fn run_investigation_step(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    context: IncidentContext,
    prompt: String,
    run_id: String,
    memory_ctx: MemoryContext,
) -> EventStream {
    Box::pin(stream! {
        let (tx, mut rx) = mpsc::unbounded_channel::<InterceptedToolCall>();
        let hook = HitlHook { tool_call_tx: tx };

        let preambles = memory_ctx.build_preambles().await;
        let agent = agents::build_investigator(&client, &config, &context, &memory_ctx, &preambles);

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

        // Check if a tool call was intercepted
        if let Ok(intercepted) = rx.try_recv() {
            match intercepted.tool_name.as_str() {
                "execute_diagnostic_command" => {
                    if let Ok(args) = serde_json::from_str::<DiagnosticCommandArgs>(&intercepted.args) {
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
                                Arc::clone(&state),
                                thread_id.clone(),
                                context,
                                forced_findings,
                                run_id.clone(),
                                memory_ctx.clone(),
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
                                Arc::clone(&state),
                                thread_id.clone(),
                                context,
                                forced_findings,
                                run_id.clone(),
                                memory_ctx.clone(),
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
                }
                "ask_user" => {
                    if let Ok(args) = serde_json::from_str::<AskUserArgs>(&intercepted.args) {
                        let pending = PendingInterrupt::incident_question(
                            args.question.clone(),
                            args.options.clone(),
                            context,
                            intercepted.tool_call_id,
                            serde_json::from_str(&intercepted.args).ok(),
                        );
                        let _ = state.store_interrupt(&thread_id, pending).await;

                        yield Ok(AgentEvent::updates_with_interrupt(
                            HitlMarker::Question {
                                question: args.question,
                                options: args.options,
                            }.into()
                        ));
                        return;
                    }
                }
                _ => {}
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
                    Arc::clone(&state),
                    thread_id.clone(),
                    context,
                    findings,
                    run_id.clone(),
                    memory_ctx.clone(),
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
///
/// After the reporter saves the post-mortem, chains to a plan confirmation
/// HITL gate instead of completing the pipeline.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields required to drive analyst/reporter + state for plan gate"
)]
fn run_analysis_and_report(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    context: IncidentContext,
    investigation_findings: String,
    run_id: String,
    memory_ctx: MemoryContext,
) -> EventStream {
    Box::pin(stream! {
        // Phase 2 — Analysis
        yield Ok(AgentEvent::phase(IncidentPhase::Analyzing));

        let preambles = memory_ctx.build_preambles().await;

        let analyst_prompt = format!(
            "Analyse the following incident evidence and provide a structured analysis.\n\n\
             Evidence:\n{}\n\n\
             Investigator findings:\n{}",
            context.to_prompt_json(),
            investigation_findings
        );

        let analyst = agents::build_analyst(&client, &config, &context, &preambles);
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

        let (reporter, report_slot) = agents::build_reporter(&client, &config, &context, &analysis_text, &preambles);
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
            Ok(_response) => {
                tracing::info!(run_id = %run_id, "ReporterAgent finished");

                // Read report path from the shared slot (deposited by SaveReportTool)
                let report_path = report_slot
                    .read()
                    .await
                    .as_ref()
                    .map(|r| r.path.clone())
                    .unwrap_or_default();

                if !report_path.is_empty() {
                    let msg = format!("Post-mortem saved to {report_path}");
                    yield Ok(AgentEvent::Message(MessageEvent::assistant(&msg)));
                }

                // Instead of completing, ask if the user wants to create a plan
                let question = "Would you like to create a remediation plan to fix this issue?".to_string();
                let options = vec![
                    "Yes, create plan".to_string(),
                    "No, skip".to_string(),
                ];

                let pending = PendingInterrupt::incident_plan_confirmation(
                    context,
                    analysis_text.clone(),
                    report_path,
                );
                let _ = state.store_interrupt(&thread_id, pending).await;

                yield Ok(AgentEvent::updates_with_interrupt(
                    HitlMarker::Question {
                        question,
                        options: Some(options),
                    }
                    .into(),
                ));
            }
            Err(e) => {
                tracing::error!(run_id = %run_id, error = ?e, "ReporterAgent failed");
                yield Err(AgentError::Other(anyhow::anyhow!("Reporter error: {}", e)));
            }
        }
    })
}

/// Run a single planning turn with the PlannerAgent.
///
/// If the agent calls `ask_user`, intercepts it and stores an
/// `IncidentPlannerQuestion` interrupt, then returns.
/// If the agent calls `save_remediation_plan`, it executes (not intercepted),
/// then chains to the review loop.
/// If the agent returns text without a tool call, it means the plan was saved
/// and we proceed to the review loop.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields required to drive planner agent + state + memory"
)]
pub(super) fn run_planning_step(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    context: IncidentContext,
    analysis_text: String,
    prompt: String,
    revision_round: usize,
    run_id: String,
    memory_ctx: MemoryContext,
) -> EventStream {
    Box::pin(stream! {
        let (tx, mut rx) = mpsc::unbounded_channel::<InterceptedToolCall>();
        let hook = HitlHook { tool_call_tx: tx };

        let preambles = memory_ctx.build_preambles().await;
        let (agent, plan_slot) = agents::build_planner(
            &client, &config, &context, &analysis_text, &memory_ctx, &preambles,
        );

        tracing::info!(
            thread_id = %thread_id,
            run_id = %run_id,
            revision_round,
            "Running PlannerAgent turn"
        );

        let result = agent
            .prompt(&prompt)
            .max_turns(3) // Allow save_remediation_plan tool to execute
            .with_hook(hook)
            .await;

        // Check if ask_user was intercepted
        if let Ok(intercepted) = rx.try_recv() {
            if intercepted.tool_name == "ask_user"
                && let Ok(args) = serde_json::from_str::<AskUserArgs>(&intercepted.args)
            {
                let pending = PendingInterrupt::incident_planner_question(
                    args.question.clone(),
                    args.options.clone(),
                    context,
                    analysis_text,
                    revision_round,
                    false, // is_review (scoping question from PlannerAgent)
                    None,  // plan_content
                    None,  // plan_path
                    intercepted.tool_call_id,
                    serde_json::from_str(&intercepted.args).ok(),
                );
                let _ = state.store_interrupt(&thread_id, pending).await;

                yield Ok(AgentEvent::updates_with_interrupt(
                    HitlMarker::Question {
                        question: args.question,
                        options: args.options,
                    }
                    .into(),
                ));
                return;
            }
            tracing::warn!(
                tool = %intercepted.tool_name,
                "Unexpected tool intercepted in planner"
            );
        }

        // No tool intercepted — plan should have been saved
        match result {
            Ok(_response) => {
                tracing::info!(run_id = %run_id, "PlannerAgent finished");

                // Read plan path from the shared slot (deposited by SavePlanTool)
                let plan_path = plan_slot
                    .read()
                    .await
                    .as_ref()
                    .map(|r| r.path.clone())
                    .unwrap_or_default();

                if plan_path.is_empty() {
                    tracing::error!(run_id = %run_id, "SavePlanTool was not called — plan not saved");
                    yield Err(AgentError::Other(anyhow::anyhow!(
                        "Planner did not save the plan"
                    )));
                    return;
                }

                // Read the plan from disk for the review loop
                match tokio::fs::read_to_string(&plan_path).await {
                    Ok(plan_content) => {
                        for await event in start_plan_review(
                            Arc::clone(&client),
                            Arc::clone(&config),
                            Arc::clone(&state),
                            thread_id.clone(),
                            context,
                            plan_content,
                            plan_path,
                            analysis_text,
                            revision_round,
                            run_id.clone(),
                            memory_ctx.clone(),
                        ) {
                            yield event;
                        }
                    }
                    Err(e) => {
                        tracing::error!(run_id = %run_id, error = ?e, "Failed to read plan file");
                        yield Err(AgentError::Other(anyhow::anyhow!(
                            "Failed to read plan: {}", e
                        )));
                    }
                }
            }
            Err(e) => {
                tracing::error!(run_id = %run_id, error = ?e, "PlannerAgent failed");
                yield Err(AgentError::Other(anyhow::anyhow!("Planner error: {}", e)));
            }
        }
    })
}

/// Show the plan to the operator and ask for changes.
///
/// Part of the review loop: shows plan content, asks if changes are needed.
/// If yes, re-runs the PlannerAgent with feedback. If no, proceeds to
/// execution confirmation.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields required for review loop context"
)]
pub fn start_plan_review(
    _client: Arc<anthropic::Client>,
    _config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    context: IncidentContext,
    plan_content: String,
    plan_path: String,
    analysis_text: String,
    revision_round: usize,
    run_id: String,
    _memory_ctx: MemoryContext,
) -> EventStream {
    Box::pin(stream! {
        tracing::info!(
            run_id = %run_id,
            plan_path = %plan_path,
            revision_round,
            "Presenting plan for operator review"
        );

        // Show plan content to operator
        let plan_message = format!(
            "**Remediation plan saved to `{}`:**\n\n{}",
            plan_path, plan_content
        );
        yield Ok(AgentEvent::Message(MessageEvent::assistant(&plan_message)));

        if revision_round >= MAX_PLAN_REVISIONS {
            tracing::warn!(
                run_id = %run_id,
                revision_round,
                "Max plan revisions reached, proceeding to execution confirmation"
            );
            yield Ok(AgentEvent::Message(MessageEvent::assistant(
                "Maximum revision rounds reached. Proceeding to execution confirmation."
            )));
        }

        // Ask if changes are needed
        let question = if revision_round >= MAX_PLAN_REVISIONS {
            "Do you want to execute this plan?".to_string()
        } else {
            "Would you like to change anything in the plan?".to_string()
        };

        let options = if revision_round >= MAX_PLAN_REVISIONS {
            vec!["Yes, execute the plan".to_string(), "No, skip execution".to_string()]
        } else {
            vec!["Yes, I want changes".to_string(), "No, proceed to execution".to_string()]
        };

        if revision_round >= MAX_PLAN_REVISIONS {
            // Max revisions — go directly to execution confirmation
            let pending = PendingInterrupt::incident_execution_confirmation(
                context,
                plan_content,
                plan_path,
            );
            let _ = state.store_interrupt(&thread_id, pending).await;

            yield Ok(AgentEvent::updates_with_interrupt(
                HitlMarker::Question {
                    question,
                    options: Some(options),
                }
                .into(),
            ));
        } else {
            // Normal review — ask for changes (carry plan content for the orchestrator)
            let pending = PendingInterrupt::incident_planner_question(
                question.clone(),
                Some(options.clone()),
                context,
                analysis_text,
                revision_round,
                true, // is_review
                Some(plan_content),
                Some(plan_path),
                None,
                None,
            );
            let _ = state.store_interrupt(&thread_id, pending).await;

            yield Ok(AgentEvent::updates_with_interrupt(
                HitlMarker::Question {
                    question,
                    options: Some(options),
                }
                .into(),
            ));
        }
    })
}

/// Run a single execution turn with the ExecutorAgent.
///
/// If the agent calls `execute_diagnostic_command`, intercepts it and stores
/// an `IncidentPlanCommand` interrupt, then returns.
/// If the agent calls `ask_user`, intercepts it and stores an
/// `IncidentExecutorQuestion` interrupt, then returns.
/// If the agent returns text (no tool call), execution is complete.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields required to drive executor agent + state + memory"
)]
fn run_execution_step(
    client: Arc<anthropic::Client>,
    config: Arc<RigAgentConfig>,
    state: Arc<StateStore>,
    thread_id: crate::agent::shared::ThreadId,
    plan_content: String,
    plan_path: String,
    prompt: String,
    run_id: String,
    memory_ctx: MemoryContext,
) -> EventStream {
    Box::pin(stream! {
        let (tx, mut rx) = mpsc::unbounded_channel::<InterceptedToolCall>();
        let hook = HitlHook { tool_call_tx: tx };

        let preambles = memory_ctx.build_preambles().await;
        let agent = agents::build_executor(
            &client, &config, &plan_content, &memory_ctx, &preambles,
        );

        tracing::info!(
            thread_id = %thread_id,
            run_id = %run_id,
            "Running ExecutorAgent turn"
        );

        let result = agent
            .prompt(&prompt)
            .max_turns(1)
            .with_hook(hook)
            .await;

        // Check if a tool call was intercepted
        if let Ok(intercepted) = rx.try_recv() {
            match intercepted.tool_name.as_str() {
                "execute_diagnostic_command" => {
                    if let Ok(args) = serde_json::from_str::<DiagnosticCommandArgs>(
                        &intercepted.args,
                    ) {
                        let hitl_message = format_hitl_message(&args);
                        let pending = PendingInterrupt::incident_plan_command(
                            args.command.clone(),
                            args.motivation.clone(),
                            args.needs_continuation,
                            args.risk_level,
                            args.expected_diagnostic_value.clone(),
                            plan_content,
                            plan_path,
                            intercepted.tool_call_id,
                            serde_json::from_str(&intercepted.args).ok(),
                        );
                        let _ = state.store_interrupt(&thread_id, pending).await;

                        yield Ok(AgentEvent::updates_with_interrupt(
                            HitlMarker::CommandApproval {
                                command: args.command,
                                message: hitl_message,
                                needs_continuation: args.needs_continuation,
                            }
                            .into(),
                        ));
                        return;
                    }
                }
                "ask_user" => {
                    if let Ok(args) = serde_json::from_str::<AskUserArgs>(&intercepted.args) {
                        let pending = PendingInterrupt::incident_executor_question(
                            args.question.clone(),
                            args.options.clone(),
                            plan_content,
                            plan_path,
                            intercepted.tool_call_id,
                            serde_json::from_str(&intercepted.args).ok(),
                        );
                        let _ = state.store_interrupt(&thread_id, pending).await;

                        yield Ok(AgentEvent::updates_with_interrupt(
                            HitlMarker::Question {
                                question: args.question,
                                options: args.options,
                            }
                            .into(),
                        ));
                        return;
                    }
                }
                _ => {}
            }
            tracing::warn!(
                tool = %intercepted.tool_name,
                "Unexpected tool intercepted in executor"
            );
        }

        // No tool intercepted — execution complete
        match result {
            Ok(summary) => {
                tracing::info!(run_id = %run_id, "ExecutorAgent finished");

                if !summary.trim().is_empty() {
                    yield Ok(AgentEvent::Message(MessageEvent::assistant(&summary)));
                }

                yield Ok(AgentEvent::phase(IncidentPhase::Completed));
                yield Ok(AgentEvent::end());
            }
            Err(e) => {
                tracing::error!(run_id = %run_id, error = ?e, "ExecutorAgent failed");
                yield Err(AgentError::Other(anyhow::anyhow!("Executor error: {}", e)));
            }
        }
    })
}
