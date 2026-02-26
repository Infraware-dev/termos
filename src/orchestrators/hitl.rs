//! Human-in-the-Loop (HITL) helpers for command approval and question handling.
//!
//! This module provides utility functions for parsing user responses to LLM interrupts.

/// Parse user input for command approval.
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
/// use infraware_terminal::orchestrators::hitl::parse_approval;
///
/// assert!(parse_approval(""));      // Enter = approve
/// assert!(parse_approval("y"));
/// assert!(parse_approval("YES"));
/// assert!(!parse_approval("n"));
/// assert!(!parse_approval("no"));
/// assert!(!parse_approval("maybe"));
/// ```
pub fn parse_approval(input: &str) -> bool {
    let trimmed = input.trim().to_lowercase();
    trimmed.is_empty() || trimmed == "y" || trimmed == "yes"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_approval_empty_is_yes() {
        // Empty string (just Enter) should approve (like Python backend)
        assert!(parse_approval(""));
        assert!(parse_approval("   "));
        assert!(parse_approval("\t"));
        assert!(parse_approval("\n"));
    }

    #[test]
    fn test_parse_approval_yes_variants() {
        assert!(parse_approval("y"));
        assert!(parse_approval("Y"));
        assert!(parse_approval("yes"));
        assert!(parse_approval("YES"));
        assert!(parse_approval("Yes"));
        assert!(parse_approval("  y  "));
        assert!(parse_approval("  yes  "));
    }

    #[test]
    fn test_parse_approval_no_variants() {
        assert!(!parse_approval("n"));
        assert!(!parse_approval("N"));
        assert!(!parse_approval("no"));
        assert!(!parse_approval("NO"));
        assert!(!parse_approval("No"));
        assert!(!parse_approval("  n  "));
        assert!(!parse_approval("  no  "));
    }

    #[test]
    fn test_parse_approval_rejects_other_input() {
        assert!(!parse_approval("maybe"));
        assert!(!parse_approval("sure"));
        assert!(!parse_approval("ok"));
        assert!(!parse_approval("yep"));
        assert!(!parse_approval("nope"));
        assert!(!parse_approval("cancel"));
        assert!(!parse_approval("abort"));
        assert!(!parse_approval("1"));
        assert!(!parse_approval("0"));
    }

    #[test]
    fn test_parse_approval_partial_matches() {
        // "ye" is not "yes", should reject
        assert!(!parse_approval("ye"));
        // "yess" is not "yes", should reject
        assert!(!parse_approval("yess"));
        // "y " with trailing space is still "y" after trim
        assert!(parse_approval("y "));
    }
}
