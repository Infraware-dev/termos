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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_approval_empty_is_yes() {
        // Empty string (just Enter) should approve (like Python backend)
        assert!(HitlOrchestrator::parse_approval(""));
        assert!(HitlOrchestrator::parse_approval("   "));
        assert!(HitlOrchestrator::parse_approval("\t"));
        assert!(HitlOrchestrator::parse_approval("\n"));
    }

    #[test]
    fn test_parse_approval_yes_variants() {
        assert!(HitlOrchestrator::parse_approval("y"));
        assert!(HitlOrchestrator::parse_approval("Y"));
        assert!(HitlOrchestrator::parse_approval("yes"));
        assert!(HitlOrchestrator::parse_approval("YES"));
        assert!(HitlOrchestrator::parse_approval("Yes"));
        assert!(HitlOrchestrator::parse_approval("  y  "));
        assert!(HitlOrchestrator::parse_approval("  yes  "));
    }

    #[test]
    fn test_parse_approval_no_variants() {
        assert!(!HitlOrchestrator::parse_approval("n"));
        assert!(!HitlOrchestrator::parse_approval("N"));
        assert!(!HitlOrchestrator::parse_approval("no"));
        assert!(!HitlOrchestrator::parse_approval("NO"));
        assert!(!HitlOrchestrator::parse_approval("No"));
        assert!(!HitlOrchestrator::parse_approval("  n  "));
        assert!(!HitlOrchestrator::parse_approval("  no  "));
    }

    #[test]
    fn test_parse_approval_rejects_other_input() {
        assert!(!HitlOrchestrator::parse_approval("maybe"));
        assert!(!HitlOrchestrator::parse_approval("sure"));
        assert!(!HitlOrchestrator::parse_approval("ok"));
        assert!(!HitlOrchestrator::parse_approval("yep"));
        assert!(!HitlOrchestrator::parse_approval("nope"));
        assert!(!HitlOrchestrator::parse_approval("cancel"));
        assert!(!HitlOrchestrator::parse_approval("abort"));
        assert!(!HitlOrchestrator::parse_approval("1"));
        assert!(!HitlOrchestrator::parse_approval("0"));
    }

    #[test]
    fn test_parse_approval_partial_matches() {
        // "ye" is not "yes", should reject
        assert!(!HitlOrchestrator::parse_approval("ye"));
        // "yess" is not "yes", should reject
        assert!(!HitlOrchestrator::parse_approval("yess"));
        // "y " with trailing space is still "y" after trim
        assert!(HitlOrchestrator::parse_approval("y "));
    }

    #[test]
    fn test_hitl_orchestrator_debug() {
        let orchestrator = HitlOrchestrator;
        let debug_str = format!("{:?}", orchestrator);
        assert!(debug_str.contains("HitlOrchestrator"));
    }
}
