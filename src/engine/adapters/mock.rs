//! Mock engine for testing

mod workflow;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_stream::stream;
use async_trait::async_trait;
use futures::stream as futures_stream;
use tokio::sync::RwLock;

use crate::engine::adapters::mock::workflow::Playbook;
pub use crate::engine::adapters::mock::workflow::Workflow;
use crate::engine::error::EngineError;
use crate::engine::traits::{AgenticEngine, EventStream};
use crate::engine::types::{HealthStatus, ResumeResponse};
use crate::engine::{AgentEvent, Interrupt, Message, MessageRole, RunInput, ThreadId};

/// Tracks progress through a workflow for resumption after interrupts
#[derive(Debug, Clone)]
struct WorkflowState {
    /// The playbook being executed
    playbook: Playbook,
    /// Current phase index (0-indexed)
    phase_index: usize,
    /// Current step index within the phase (0-indexed)
    step_index: usize,
    /// Duration to wait before showing step completion (simulates command execution)
    step_duration: Duration,
}

impl WorkflowState {
    /// Calculate the step duration for a phase based on its `duration_minutes` setting.
    ///
    /// If the phase has a `duration_minutes`, the total duration is divided by the number of steps.
    /// Otherwise, defaults to 2 seconds per step.
    fn calculate_step_duration(playbook: &Playbook, phase_index: usize) -> Duration {
        let phase = &playbook.phases[phase_index];
        let steps = phase.steps.as_ref().map(|s| s.len()).unwrap_or(1);

        let duration = match phase.duration_minutes {
            Some(minutes) if steps > 0 => Duration::from_secs((minutes * 60) / steps as u64),
            _ => Duration::from_secs(2),
        };

        tracing::debug!(
            phase_name = %phase.name,
            phase_index,
            step_count = steps,
            duration_secs = duration.as_secs(),
            "Calculated step duration for phase"
        );

        duration
    }
}

/// Mock engine that returns canned responses
///
/// Useful for testing the API layer without a real agent backend.
#[derive(Debug)]
pub struct MockEngine {
    /// Counter for generating unique thread IDs
    thread_counter: AtomicU64,
    /// Stored threads (thread_id -> messages)
    threads: Arc<RwLock<HashMap<String, Vec<Message>>>>,
    /// Pending interrupt for a thread (for testing HITL)
    pending_interrupts: Arc<RwLock<HashMap<String, Interrupt>>>,
    /// Pending workflow state for resumption after interrupts
    pending_workflow_state: Arc<RwLock<HashMap<String, WorkflowState>>>,
    /// The workflow to use for returning mocked responses
    workflow: Workflow,
}

impl Default for MockEngine {
    fn default() -> Self {
        Self::new(None)
    }
}

impl MockEngine {
    pub fn new(workflow: Option<Workflow>) -> Self {
        let has_custom_workflow = workflow.is_some();
        let workflow = workflow.unwrap_or_else(self::workflow::mock_workflow);

        tracing::debug!(
            has_custom_workflow,
            playbook_count = workflow.playbooks.len(),
            playbook_keys = ?workflow.playbooks.keys().collect::<Vec<_>>(),
            "Initializing MockEngine"
        );

        Self {
            thread_counter: AtomicU64::new(1),
            threads: Arc::new(RwLock::new(HashMap::new())),
            pending_interrupts: Arc::new(RwLock::new(HashMap::new())),
            pending_workflow_state: Arc::new(RwLock::new(HashMap::new())),
            workflow,
        }
    }

    /// Queue an interrupt for the next run on a thread (for testing)
    pub async fn queue_interrupt(&self, thread_id: &ThreadId, interrupt: Interrupt) {
        tracing::debug!(
            thread_id = %thread_id,
            interrupt = ?interrupt,
            "Queuing test interrupt for next run"
        );

        self.pending_interrupts
            .write()
            .await
            .insert(thread_id.0.clone(), interrupt);
    }

    /// Match input to a playbook in the workflow
    ///
    /// This is done by iterating over playbooks and checking if any intent is contained in the input.
    fn match_playbook_from_input(&self, input: &str) -> Option<Playbook> {
        tracing::debug!(input = %input, "Attempting to match input to playbook");

        let result = self.workflow.playbooks.iter().find_map(|(key, playbook)| {
            let matched_intent = playbook
                .intents
                .iter()
                .find(|intent| input.to_lowercase().contains(&intent.to_lowercase()));

            if let Some(intent) = matched_intent {
                tracing::debug!(
                    playbook_key = %key,
                    matched_intent = %intent,
                    phase_count = playbook.phases.len(),
                    "Matched playbook by intent"
                );
                Some(playbook.clone())
            } else {
                None
            }
        });

        if result.is_none() {
            tracing::debug!(
                available_playbooks = ?self.workflow.playbooks.keys().collect::<Vec<_>>(),
                "No playbook matched input"
            );
        }

        result
    }

    /// Store workflow state for later resumption
    async fn store_workflow_state(&self, thread_id: &ThreadId, state: WorkflowState) {
        tracing::debug!(
            thread_id = %thread_id,
            phase_index = state.phase_index,
            step_index = state.step_index,
            step_duration_secs = state.step_duration.as_secs(),
            "Storing workflow state for resumption"
        );

        self.pending_workflow_state
            .write()
            .await
            .insert(thread_id.0.clone(), state);
    }

    /// Take (retrieve and remove) workflow state for a thread
    async fn take_workflow_state(&self, thread_id: &ThreadId) -> Option<WorkflowState> {
        let state = self
            .pending_workflow_state
            .write()
            .await
            .remove(&thread_id.0);

        match &state {
            Some(s) => tracing::debug!(
                thread_id = %thread_id,
                phase_index = s.phase_index,
                step_index = s.step_index,
                "Retrieved and removed workflow state"
            ),
            None => tracing::debug!(
                thread_id = %thread_id,
                "No workflow state found for thread"
            ),
        }

        state
    }

    /// Build events for starting at a specific step in a workflow (with interrupt for approval).
    ///
    /// Returns events up to and including the interrupt for the current step,
    /// plus the workflow state to store for resumption.
    fn build_step_interrupt_events(
        playbook: &Playbook,
        phase_index: usize,
        step_index: usize,
        run_id: &str,
    ) -> (Vec<Result<AgentEvent, EngineError>>, WorkflowState) {
        let phase = &playbook.phases[phase_index];
        let steps = phase.steps.as_ref().expect("Phase must have steps");
        let step = &steps[step_index];

        tracing::debug!(
            run_id = %run_id,
            phase_name = %phase.name,
            phase_index,
            step_index,
            command = %step.command,
            action = %step.action,
            "Building step interrupt events"
        );

        let events = vec![
            Ok(AgentEvent::metadata(run_id)),
            // Show what action we're about to take
            Ok(AgentEvent::Values {
                messages: vec![Message::assistant(&step.action)],
            }),
            // Request approval for the command
            Ok(AgentEvent::updates_with_interrupt(
                Interrupt::CommandApproval {
                    command: step.command.clone(),
                    message: phase.name.clone(),
                    needs_continuation: false,
                },
            )),
        ];

        let state = WorkflowState {
            playbook: playbook.clone(),
            phase_index,
            step_index,
            step_duration: WorkflowState::calculate_step_duration(playbook, phase_index),
        };

        (events, state)
    }

    /// Build events for the entire workflow without interrupts.
    ///
    /// Used when `run_commands` is false - returns all step info as messages
    /// without requiring user approval. Includes delays to simulate command execution.
    fn build_workflow_events_no_interrupt(playbook: Playbook, run_id: String) -> EventStream {
        tracing::debug!(
            run_id = %run_id,
            playbook_name = %playbook.name,
            phase_count = playbook.phases.len(),
            "Building workflow events without interrupts (run_commands=false)"
        );

        Box::pin(stream! {
            yield Ok(AgentEvent::metadata(&run_id));

            // Accumulate content - client expects full state in each values event
            let mut accumulated = String::new();

            /// Helper macro to append content and yield accumulated state
            macro_rules! emit {
                ($content:expr) => {{
                    if !accumulated.is_empty() {
                        accumulated.push_str("\n\n");
                    }
                    accumulated.push_str(&$content);
                    tracing::debug!(
                        accumulated_len = accumulated.len(),
                        "Emitting accumulated content (run_commands=false)"
                    );
                    yield Ok(AgentEvent::Values {
                        messages: vec![Message::assistant(&accumulated)],
                    });
                }};
            }

            for phase in &playbook.phases {
                // Calculate step duration for this phase
                let step_count = phase.steps.as_ref().map(|s| s.len()).unwrap_or(1);
                let step_duration = match phase.duration_minutes {
                    Some(minutes) if step_count > 0 => {
                        Duration::from_secs((minutes * 60) / step_count as u64)
                    }
                    _ => Duration::from_secs(2),
                };

                // Add phase description
                emit!(format!(
                    "**Phase {}: {}**\n{}",
                    phase.phase, phase.name, phase.description
                ));

                // Process steps if present
                if let Some(steps) = &phase.steps {
                    for step in steps {
                        // Show action
                        emit!(step.action.clone());

                        // Show command being "executed"
                        emit!(format!("```\n$ {}\n```", step.command));

                        // Simulate command execution time
                        tracing::debug!(
                            step_duration_ms = step_duration.as_millis(),
                            "Sleeping to simulate command execution"
                        );
                        tokio::time::sleep(step_duration).await;
                        tracing::debug!("Sleep completed, continuing workflow");

                        // Show output (as code block)
                        emit!(format!("```\n{}\n```", step.output));

                        // Show analysis
                        emit!(step.analysis.clone());
                    }
                }

                // Add root cause if present
                if let Some(root_cause) = &phase.root_cause {
                    emit!(format!(
                        "**Root Cause Identified**\n- Issue: {}\n- Impact: {}\n- Drift Type: {}",
                        root_cause.issue, root_cause.impact, root_cause.drift_type
                    ));
                }

                // Add verification summary if present
                if let Some(summary) = &phase.verification_summary {
                    let summary_text = summary
                        .iter()
                        .map(|(k, v)| format!("- {}: {}", k, v))
                        .collect::<Vec<_>>()
                        .join("\n");
                    emit!(format!("**Verification Summary**\n{}", summary_text));
                }

                // Add phase conclusion if present
                if let Some(conclusion) = &phase.conclusion {
                    emit!(conclusion.clone());
                }
            }

            // Only show completion message if the last phase didn't have a conclusion
            let last_phase_has_conclusion = playbook
                .phases
                .last()
                .is_some_and(|p| p.conclusion.is_some());
            if !last_phase_has_conclusion {
                emit!(String::from("Workflow completed successfully."));
            }
            yield Ok(AgentEvent::end());
        })
    }

    /// Find the next step with a command in the workflow starting from the given position.
    ///
    /// Returns `Some((phase_index, step_index))` if found, `None` if workflow is complete.
    fn find_next_step(
        playbook: &Playbook,
        from_phase: usize,
        from_step: usize,
    ) -> Option<(usize, usize)> {
        tracing::debug!(
            from_phase,
            from_step,
            total_phases = playbook.phases.len(),
            "Searching for next step in workflow"
        );

        for phase_idx in from_phase..playbook.phases.len() {
            let phase = &playbook.phases[phase_idx];

            // Skip phases without steps (root_cause, verification_summary)
            let Some(steps) = &phase.steps else {
                tracing::debug!(
                    phase_idx,
                    phase_name = %phase.name,
                    "Skipping phase without steps"
                );
                continue;
            };

            let start_step = if phase_idx == from_phase {
                from_step
            } else {
                0
            };

            if start_step < steps.len() {
                tracing::debug!(
                    phase_idx,
                    step_idx = start_step,
                    phase_name = %phase.name,
                    "Found next step"
                );
                return Some((phase_idx, start_step));
            }
        }

        tracing::debug!("No more steps found in workflow");
        None
    }

    /// Build events for completing the current step and potentially starting the next one.
    ///
    /// Returns the events and optionally the new workflow state if there's another step.
    fn build_step_completion_events(
        state: &WorkflowState,
        run_id: &str,
    ) -> (Vec<Result<AgentEvent, EngineError>>, Option<WorkflowState>) {
        let phase = &state.playbook.phases[state.phase_index];
        let steps = phase.steps.as_ref().expect("Phase must have steps");
        let step = &steps[state.step_index];

        tracing::debug!(
            run_id = %run_id,
            phase_name = %phase.name,
            phase_index = state.phase_index,
            step_index = state.step_index,
            command = %step.command,
            output_len = step.output.len(),
            "Building step completion events"
        );

        let mut events = vec![
            Ok(AgentEvent::metadata(run_id)),
            // Show the command output
            Ok(AgentEvent::Values {
                messages: vec![Message::system(&step.output)],
            }),
            // Show the analysis
            Ok(AgentEvent::Values {
                messages: vec![Message::assistant(&step.analysis)],
            }),
        ];

        // check if this is the last step inside the phase
        if state.step_index + 1 >= steps.len()
            && let Some(conclusion) = phase.conclusion.as_ref()
        {
            events.push(Ok(AgentEvent::Values {
                messages: vec![Message::assistant(conclusion)],
            }));
        }

        // Check if there's a next step
        let next_step_index = state.step_index + 1;
        if let Some((next_phase_idx, next_step_idx)) =
            Self::find_next_step(&state.playbook, state.phase_index, next_step_index)
        {
            // There's another step - add interrupt for it
            let next_phase = &state.playbook.phases[next_phase_idx];
            let next_steps = next_phase.steps.as_ref().expect("Phase must have steps");
            let next_step = &next_steps[next_step_idx];

            tracing::debug!(
                next_phase_idx,
                next_step_idx,
                next_phase_name = %next_phase.name,
                next_command = %next_step.command,
                "Workflow continues with next step"
            );

            events.push(Ok(AgentEvent::Values {
                messages: vec![Message::assistant(&next_step.action)],
            }));

            let new_state = WorkflowState {
                playbook: state.playbook.clone(),
                phase_index: next_phase_idx,
                step_index: next_step_idx,
                step_duration: WorkflowState::calculate_step_duration(
                    &state.playbook,
                    next_phase_idx,
                ),
            };

            (events, Some(new_state))
        } else {
            // Workflow complete
            tracing::debug!(
                phase_index = state.phase_index,
                step_index = state.step_index,
                "Workflow completed - no more steps"
            );

            events.push(Ok(AgentEvent::Values {
                messages: vec![Message::assistant("Workflow completed successfully.")],
            }));
            events.push(Ok(AgentEvent::end()));

            (events, None)
        }
    }
}

#[async_trait]
impl AgenticEngine for MockEngine {
    async fn create_thread(
        &self,
        _metadata: Option<serde_json::Value>,
    ) -> Result<ThreadId, EngineError> {
        let id = self.thread_counter.fetch_add(1, Ordering::SeqCst);
        let thread_id = format!("mock-thread-{}", id);

        self.threads
            .write()
            .await
            .insert(thread_id.clone(), Vec::new());

        tracing::info!(thread_id = %thread_id, "Created mock thread");
        Ok(ThreadId::new(thread_id))
    }

    async fn stream_run(
        &self,
        thread_id: &ThreadId,
        input: RunInput,
    ) -> Result<EventStream, EngineError> {
        // Check thread exists
        let mut threads = self.threads.write().await;
        let messages = threads
            .get_mut(&thread_id.0)
            .ok_or_else(|| EngineError::thread_not_found(&thread_id.0))?;

        // Store input messages
        tracing::debug!("Input messages: {:?}", input.messages);
        messages.extend(input.messages.clone());

        // Get user message content
        let user_content = input
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        // Check for pending interrupt (for testing)
        let pending = self.pending_interrupts.write().await.remove(&thread_id.0);

        let run_id = format!("mock-run-{}", uuid::Uuid::new_v4());
        tracing::info!(thread_id = %thread_id, run_id = %run_id, "Starting mock run");

        tracing::debug!(
            thread_id = %thread_id,
            user_content = %user_content,
            message_count = input.messages.len(),
            "Processing user input"
        );

        // If there's a pending test interrupt, return that
        if let Some(ref interrupt) = pending {
            tracing::debug!(
                thread_id = %thread_id,
                run_id = %run_id,
                interrupt = ?interrupt,
                "Returning pending test interrupt"
            );
            return Ok(Box::pin(futures_stream::iter(vec![
                Ok(AgentEvent::metadata(&run_id)),
                Ok(AgentEvent::updates_with_interrupt(pending.unwrap())),
            ])));
        }

        // Match user content to a playbook
        let Some(playbook) = self.match_playbook_from_input(&user_content) else {
            // No playbook matches - return generic response
            tracing::debug!(
                thread_id = %thread_id,
                run_id = %run_id,
                "No playbook matched, returning generic response"
            );
            let response = format!(
                "I understand you're asking about: \"{user_content}\". In a production environment, I would provide detailed assistance."
            );
            let assistant_msg = Message::assistant(&response);
            messages.push(assistant_msg.clone());
            let events = vec![
                Ok(AgentEvent::metadata(&run_id)),
                Ok(AgentEvent::Values {
                    messages: vec![assistant_msg],
                }),
                Ok(AgentEvent::end()),
            ];
            return Ok(Box::pin(futures_stream::iter(events)));
        };

        tracing::debug!(
            thread_id = %thread_id,
            run_id = %run_id,
            phase_count = playbook.phases.len(),
            run_commands = self.workflow.run_commands,
            "Playbook matched"
        );

        // If run_commands is false, return all workflow events without interrupts
        if !self.workflow.run_commands {
            tracing::debug!(
                thread_id = %thread_id,
                run_id = %run_id,
                "run_commands=false, returning workflow without interrupts"
            );
            return Ok(Self::build_workflow_events_no_interrupt(playbook, run_id));
        }

        // Find the first step in the workflow
        let Some((phase_idx, step_idx)) = Self::find_next_step(&playbook, 0, 0) else {
            // No steps in workflow - just end
            tracing::debug!(
                thread_id = %thread_id,
                run_id = %run_id,
                "Workflow has no executable steps"
            );
            let events = vec![
                Ok(AgentEvent::metadata(&run_id)),
                Ok(AgentEvent::Values {
                    messages: vec![Message::assistant("Workflow has no steps to execute.")],
                }),
                Ok(AgentEvent::end()),
            ];
            return Ok(Box::pin(futures_stream::iter(events)));
        };

        // Build events for the first step (including interrupt)
        let (events, state) =
            Self::build_step_interrupt_events(&playbook, phase_idx, step_idx, &run_id);

        // Store workflow state for resumption
        self.store_workflow_state(thread_id, state).await;

        // Return stream that ends after the interrupt
        Ok(Box::pin(futures_stream::iter(events)))
    }

    async fn resume_run(
        &self,
        thread_id: &ThreadId,
        response: ResumeResponse,
    ) -> Result<EventStream, EngineError> {
        // Check thread exists
        if !self.threads.read().await.contains_key(&thread_id.0) {
            return Err(EngineError::thread_not_found(&thread_id.0));
        }

        let run_id = format!("mock-run-{}", uuid::Uuid::new_v4());
        tracing::info!(thread_id = %thread_id, run_id = %run_id, ?response, "Resuming mock run");

        // Retrieve the stored workflow state
        let Some(state) = self.take_workflow_state(thread_id).await else {
            // No workflow state - return generic response (backwards compatibility)
            tracing::debug!(
                thread_id = %thread_id,
                run_id = %run_id,
                response = ?response,
                "No workflow state found, using backwards-compatible generic response"
            );

            let response_text = match response {
                ResumeResponse::Approved => {
                    "Command approved and executed successfully.".to_string()
                }
                ResumeResponse::Rejected => "Command was rejected by user.".to_string(),
                ResumeResponse::Answer { text } => {
                    format!("Received your answer: \"{}\". Processing...", text)
                }
                ResumeResponse::CommandOutput { command, output } => {
                    format!(
                        "Command `{}` executed in terminal.\nOutput ({} chars): {}",
                        command,
                        output.len(),
                        if output.len() > 100 {
                            format!("{}...", &output[..100])
                        } else {
                            output
                        }
                    )
                }
            };

            let events = vec![
                Ok(AgentEvent::metadata(&run_id)),
                Ok(AgentEvent::Values {
                    messages: vec![Message::assistant(response_text)],
                }),
                Ok(AgentEvent::end()),
            ];
            return Ok(Box::pin(futures_stream::iter(events)));
        };

        // Handle the response based on type
        match response {
            ResumeResponse::Approved | ResumeResponse::CommandOutput { .. } => {
                tracing::debug!(
                    thread_id = %thread_id,
                    run_id = %run_id,
                    phase_index = state.phase_index,
                    step_index = state.step_index,
                    sleep_duration_secs = state.step_duration.as_secs(),
                    "Command approved, simulating execution"
                );

                // Simulate command execution time
                tokio::time::sleep(state.step_duration).await;

                // User approved the command - show output and continue workflow
                let (events, new_state) = Self::build_step_completion_events(&state, &run_id);

                // Store new state if there's more work to do
                if let Some(new_state) = new_state {
                    tracing::debug!(
                        thread_id = %thread_id,
                        has_more_steps = true,
                        "Storing state for next step"
                    );
                    self.store_workflow_state(thread_id, new_state).await;
                } else {
                    tracing::debug!(
                        thread_id = %thread_id,
                        has_more_steps = false,
                        "Workflow complete after this step"
                    );
                }

                Ok(Box::pin(futures_stream::iter(events)))
            }
            ResumeResponse::Rejected => {
                tracing::debug!(
                    thread_id = %thread_id,
                    run_id = %run_id,
                    phase_index = state.phase_index,
                    step_index = state.step_index,
                    "Command rejected by user, stopping workflow"
                );

                // User rejected - end the workflow
                let events = vec![
                    Ok(AgentEvent::metadata(&run_id)),
                    Ok(AgentEvent::Values {
                        messages: vec![Message::assistant(
                            "Command rejected. Workflow stopped at user request.",
                        )],
                    }),
                    Ok(AgentEvent::end()),
                ];

                Ok(Box::pin(futures_stream::iter(events)))
            }
            ResumeResponse::Answer { ref text } => {
                tracing::debug!(
                    thread_id = %thread_id,
                    run_id = %run_id,
                    answer_len = text.len(),
                    phase_index = state.phase_index,
                    step_index = state.step_index,
                    "Received answer from user, continuing workflow"
                );

                // Simulate processing time
                tokio::time::sleep(state.step_duration).await;

                // Answer to a question - for now just acknowledge and continue
                let (events, new_state) = Self::build_step_completion_events(&state, &run_id);

                // Prepend acknowledgment of the answer
                let mut all_events = vec![
                    Ok(AgentEvent::metadata(&run_id)),
                    Ok(AgentEvent::Values {
                        messages: vec![Message::assistant(format!(
                            "Received your answer: \"{}\". Continuing...",
                            text
                        ))],
                    }),
                ];
                // Skip the metadata from build_step_completion_events since we already added it
                all_events.extend(events.into_iter().skip(1));

                if let Some(new_state) = new_state {
                    self.store_workflow_state(thread_id, new_state).await;
                }

                Ok(Box::pin(futures_stream::iter(all_events)))
            }
        }
    }

    async fn health_check(&self) -> Result<HealthStatus, EngineError> {
        let thread_count = self.threads.read().await.len();
        let pending_interrupts = self.pending_interrupts.read().await.len();
        let pending_workflows = self.pending_workflow_state.read().await.len();

        tracing::debug!(
            thread_count,
            pending_interrupts,
            pending_workflows,
            "MockEngine health check"
        );

        Ok(HealthStatus::healthy().with_details(serde_json::json!({
            "engine": "mock",
            "threads": thread_count,
            "pending_interrupts": pending_interrupts,
            "pending_workflows": pending_workflows
        })))
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::*;

    #[tokio::test]
    async fn test_create_thread() {
        let engine = MockEngine::new(None);
        let thread_id = engine.create_thread(None).await.unwrap();
        assert!(thread_id.as_str().starts_with("mock-thread-"));
    }

    #[tokio::test]
    async fn test_stream_run() {
        let engine = MockEngine::new(None);
        let thread_id = engine.create_thread(None).await.unwrap();

        let input = RunInput::single_user_message("How do I list files?");
        let mut stream = engine.stream_run(&thread_id, input).await.unwrap();

        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }

        assert!(events.len() >= 2);
        assert!(matches!(events[0], AgentEvent::Metadata { .. }));
    }

    #[tokio::test]
    async fn test_stream_run_with_interrupt() {
        let engine = MockEngine::new(None);
        let thread_id = engine.create_thread(None).await.unwrap();

        // Queue an interrupt
        engine
            .queue_interrupt(
                &thread_id,
                Interrupt::command_approval("rm -rf temp/", "Clean temp files", false),
            )
            .await;

        let input = RunInput::single_user_message("Clean up");
        let mut stream = engine.stream_run(&thread_id, input).await.unwrap();

        let mut found_interrupt = false;
        while let Some(event) = stream.next().await {
            if let AgentEvent::Updates { interrupts } = event.unwrap() {
                if interrupts.is_some() {
                    found_interrupt = true;
                }
            }
        }

        assert!(found_interrupt);
    }

    #[tokio::test]
    async fn test_resume_run() {
        let engine = MockEngine::new(None);
        let thread_id = engine.create_thread(None).await.unwrap();

        let mut stream = engine
            .resume_run(&thread_id, ResumeResponse::approved())
            .await
            .unwrap();

        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }

        assert!(!events.is_empty());
    }

    #[tokio::test]
    async fn test_health_check() {
        let engine = MockEngine::new(None);
        let status = engine.health_check().await.unwrap();
        assert!(status.healthy);
    }

    #[tokio::test]
    async fn test_thread_not_found() {
        let engine = MockEngine::new(None);
        let fake_thread = ThreadId::new("nonexistent");
        let input = RunInput::single_user_message("Hello");

        let result = engine.stream_run(&fake_thread, input).await;
        assert!(matches!(result, Err(EngineError::ThreadNotFound(_))));
    }
}
