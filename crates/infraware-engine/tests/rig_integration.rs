//! Integration tests for RigEngine
//!
//! These tests verify the end-to-end flow of the RigEngine.
//! Some tests require ANTHROPIC_API_KEY to be set.

#![cfg(feature = "rig")]

use futures::StreamExt;
use infraware_engine::adapters::{RigEngine, RigEngineConfig};
use infraware_engine::{AgenticEngine, Interrupt, ResumeResponse};
use infraware_shared::{AgentEvent, Message, RunInput};

/// Create a test configuration (uses mock values if API key not set)
fn test_config() -> RigEngineConfig {
    RigEngineConfig {
        api_key: std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| "test-api-key".to_string()),
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 1024,
        memory: Default::default(),
        system_prompt: "You are a helpful assistant.".to_string(),
        timeout_secs: 30,
        temperature: 0.7,
    }
}

/// Check if we have a real API key for live tests
fn has_real_api_key() -> bool {
    std::env::var("ANTHROPIC_API_KEY")
        .map(|k| k.starts_with("sk-ant-"))
        .unwrap_or(false)
}

#[tokio::test]
async fn test_engine_creation() {
    let config = test_config();
    let engine = RigEngine::new(config);
    assert!(engine.is_ok(), "Engine should be created successfully");
}

#[tokio::test]
async fn test_create_thread() {
    let config = test_config();
    let engine = RigEngine::new(config).unwrap();

    let thread_id = engine.create_thread(None).await;
    assert!(thread_id.is_ok(), "Thread should be created");

    let thread_id = thread_id.unwrap();
    assert!(
        !thread_id.as_str().is_empty(),
        "Thread ID should not be empty"
    );
}

#[tokio::test]
async fn test_create_multiple_threads() {
    let config = test_config();
    let engine = RigEngine::new(config).unwrap();

    let thread1 = engine.create_thread(None).await.unwrap();
    let thread2 = engine.create_thread(None).await.unwrap();
    let thread3 = engine.create_thread(None).await.unwrap();

    assert_ne!(thread1.as_str(), thread2.as_str());
    assert_ne!(thread2.as_str(), thread3.as_str());
    assert_ne!(thread1.as_str(), thread3.as_str());
}

#[tokio::test]
async fn test_health_check() {
    let config = test_config();
    let engine = RigEngine::new(config).unwrap();

    let health = engine.health_check().await;
    assert!(health.is_ok(), "Health check should succeed");

    let status = health.unwrap();
    assert!(status.healthy, "Engine should be healthy");
    // Engine info is in details, not message
    assert!(status.details.is_some(), "Should have details");
    let details = status.details.unwrap();
    assert_eq!(
        details["engine"], "rig",
        "Details should mention rig engine"
    );
}

#[tokio::test]
async fn test_stream_run_without_api_key() {
    // This test verifies the stream setup works even without a valid API key
    // The actual API call will fail, but the stream should be created
    let config = RigEngineConfig {
        api_key: "invalid-key".to_string(),
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 1024,
        memory: Default::default(),
        system_prompt: "Test".to_string(),
        timeout_secs: 30,
        temperature: 0.7,
    };

    let engine = RigEngine::new(config).unwrap();
    let thread_id = engine.create_thread(None).await.unwrap();

    let input = RunInput {
        messages: vec![Message::user("Hello")],
    };

    let stream = engine.stream_run(&thread_id, input).await;
    assert!(stream.is_ok(), "Stream should be created");

    // The first event should be metadata
    let mut stream = stream.unwrap();
    let first_event = stream.next().await;
    assert!(first_event.is_some(), "Should receive at least one event");

    match first_event.unwrap() {
        Ok(AgentEvent::Metadata { .. }) => {
            // Expected - metadata is always first
        }
        Ok(other) => panic!("Expected Metadata event, got: {:?}", other),
        Err(e) => {
            // API error is expected with invalid key
            assert!(
                e.to_string().contains("Agent error") || e.to_string().contains("error"),
                "Should be an API error"
            );
        }
    }
}

#[tokio::test]
async fn test_resume_without_pending_interrupt() {
    let config = test_config();
    let engine = RigEngine::new(config).unwrap();
    let thread_id = engine.create_thread(None).await.unwrap();

    // Try to resume without any pending interrupt
    let response = ResumeResponse::Approved;
    let stream = engine.resume_run(&thread_id, response).await;

    assert!(stream.is_ok(), "Stream should be created");

    let mut stream = stream.unwrap();

    // Should get metadata first, then an error about no pending interrupt
    let mut found_error = false;
    while let Some(event) = stream.next().await {
        match event {
            Ok(AgentEvent::Metadata { .. }) => continue,
            Err(e) => {
                assert!(
                    e.to_string().contains("not resumable"),
                    "Error should mention not resumable: {}",
                    e
                );
                found_error = true;
                break;
            }
            _ => {}
        }
    }

    assert!(found_error, "Should receive an error for missing interrupt");
}

#[tokio::test]
async fn test_thread_not_found() {
    let config = test_config();
    let engine = RigEngine::new(config).unwrap();

    let fake_thread_id = infraware_shared::ThreadId::from("nonexistent-thread-123");

    let input = RunInput {
        messages: vec![Message::user("Hello")],
    };

    let stream = engine.stream_run(&fake_thread_id, input).await;
    assert!(stream.is_err(), "Should fail for nonexistent thread");
}

// Live API tests - only run when ANTHROPIC_API_KEY is set
mod live_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires ANTHROPIC_API_KEY"]
    async fn test_live_simple_query() {
        if !has_real_api_key() {
            println!("Skipping live test - no API key");
            return;
        }

        let config = test_config();
        let engine = RigEngine::new(config).unwrap();
        let thread_id = engine.create_thread(None).await.unwrap();

        let input = RunInput {
            messages: vec![Message::user("Say hello in one word.")],
        };

        let mut stream = engine.stream_run(&thread_id, input).await.unwrap();

        let mut received_message = false;
        let mut received_end = false;

        while let Some(event) = stream.next().await {
            match event {
                Ok(AgentEvent::Metadata { .. }) => {}
                Ok(AgentEvent::Message(_)) => received_message = true,
                Ok(AgentEvent::End { .. }) => received_end = true,
                Ok(AgentEvent::Values { .. }) => {}
                Ok(AgentEvent::Updates { .. }) => {}
                Ok(AgentEvent::Error { message }) => panic!("Unexpected error: {}", message),
                Err(e) => panic!("Unexpected error: {}", e),
            }
        }

        assert!(received_message, "Should receive a message");
        assert!(received_end, "Should receive end event");
    }

    #[tokio::test]
    #[ignore = "Requires ANTHROPIC_API_KEY"]
    async fn test_live_command_approval_flow() {
        if !has_real_api_key() {
            println!("Skipping live test - no API key");
            return;
        }

        let config = RigEngineConfig {
            api_key: std::env::var("ANTHROPIC_API_KEY").unwrap(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 1024,
            memory: Default::default(),
            system_prompt: "You are a DevOps assistant. When the user asks to run a command, use the shell_command tool.".to_string(),
            timeout_secs: 60,
            temperature: 0.0, // Deterministic for testing
        };

        let engine = RigEngine::new(config).unwrap();
        let thread_id = engine.create_thread(None).await.unwrap();

        let input = RunInput {
            messages: vec![Message::user("Please run: ls -la")],
        };

        let mut stream = engine.stream_run(&thread_id, input).await.unwrap();

        let mut received_interrupt = false;

        while let Some(event) = stream.next().await {
            match event {
                Ok(AgentEvent::Updates { interrupts, .. }) => {
                    if let Some(ints) = interrupts {
                        for interrupt in &ints {
                            if let Interrupt::CommandApproval { command, .. } = interrupt {
                                received_interrupt = true;
                                assert!(
                                    command.contains("ls"),
                                    "Command should contain ls: {}",
                                    command
                                );
                                break;
                            }
                        }
                    }
                    if received_interrupt {
                        break;
                    }
                }
                Ok(AgentEvent::Error { message }) => panic!("Unexpected error: {}", message),
                Err(e) => panic!("Unexpected error: {}", e),
                _ => {}
            }
        }

        assert!(
            received_interrupt,
            "Should receive command approval interrupt"
        );
    }
}
