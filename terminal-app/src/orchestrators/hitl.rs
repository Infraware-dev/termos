//! Human-in-the-Loop (HITL) orchestrator for command approval and question handling
//!
//! This module centralizes HITL logic for parsing user responses to LLM interrupts.

/// Human-in-the-Loop orchestrator
///
/// Provides helper methods for parsing and validating HITL responses.
/// The actual handling of approvals/answers is done by `NaturalLanguageOrchestrator`,
/// but this module centralizes the parsing logic.
#[derive(Debug)]
pub struct HitlOrchestrator;

impl HitlOrchestrator {
    /// Parse user input for command approval
    ///
    /// Returns `true` for approval, `false` for rejection.
    ///
    /// Approval inputs (case-insensitive):
    /// - Empty string (just pressing Enter) - default approve like Python backend
    /// - "y" or "yes"
    ///
    /// Rejection inputs:
    /// - "n", "no", or any other input
    ///
    /// # Example
    /// ```
    /// use infraware_terminal::orchestrators::HitlOrchestrator;
    ///
    /// assert!(HitlOrchestrator::parse_approval(""));      // Enter = approve
    /// assert!(HitlOrchestrator::parse_approval("y"));
    /// assert!(HitlOrchestrator::parse_approval("YES"));
    /// assert!(!HitlOrchestrator::parse_approval("n"));
    /// assert!(!HitlOrchestrator::parse_approval("no"));
    /// assert!(!HitlOrchestrator::parse_approval("maybe"));
    /// ```
    pub fn parse_approval(input: &str) -> bool {
        let trimmed = input.trim().to_lowercase();
        trimmed.is_empty() || trimmed == "y" || trimmed == "yes"
    }
}
