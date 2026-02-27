//! Shared shell command execution primitives.
use std::process::Stdio;

use regex::Regex;
use serde::Deserialize;
use tokio::process::Command;
use tokio::time::{Duration, timeout};

/// Fixture format for simulation mode (activated via `SIM_FIXTURE` env var).
///
/// Re-read on every simulated command call to allow hot-reload during demo.
/// Pattern fields are full regex strings matched against the full command string.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimFixture {
    /// Optional human-readable scenario description.
    #[allow(dead_code)]
    #[serde(default)]
    description: Option<String>,
    command_responses: Vec<SimEntry>,
    #[serde(default = "default_fallback")]
    fallback_output: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimEntry {
    pattern: String,
    output: String,
}

fn default_fallback() -> String {
    "(SIM) Command not matched in fixture".to_string()
}

/// Return a canned response from the fixture file, matching `command` against
/// each entry's `pattern` regex in order. Falls back to `fallback_output` if
/// no pattern matches.
///
/// Fixture is re-read on every call intentionally: enables hot-reload
/// during interactive demos without restarting the server.
fn simulate_command(command: &str, fixture_path: &str) -> String {
    let Ok(content) = std::fs::read_to_string(fixture_path) else {
        tracing::warn!(
            command = %command,
            fixture_path = %fixture_path,
            "SIM: failed to read fixture file"
        );
        return format!("SIM: failed to read fixture '{fixture_path}'");
    };
    let Ok(fixture) = serde_json::from_str::<SimFixture>(&content) else {
        tracing::warn!(
            command = %command,
            fixture_path = %fixture_path,
            "SIM: fixture JSON parse error"
        );
        return format!("SIM: invalid fixture JSON at '{fixture_path}'");
    };
    tracing::info!(
        command = %command,
        fixture_path = %fixture_path,
        patterns = fixture.command_responses.len(),
        "SIM: evaluating command against fixture patterns"
    );
    for entry in &fixture.command_responses {
        let Ok(re) = Regex::new(&entry.pattern) else {
            tracing::warn!(pattern = %entry.pattern, "SIM: invalid regex in fixture — skipping entry");
            continue;
        };
        if re.is_match(command) {
            tracing::info!(
                command = %command,
                pattern = %entry.pattern,
                output_len = entry.output.len(),
                "SIM: matched fixture pattern"
            );
            return entry.output.clone();
        }
    }
    tracing::info!(
        command = %command,
        fallback_len = fixture.fallback_output.len(),
        "SIM: no fixture pattern matched, returning fallback output"
    );
    fixture.fallback_output
}

/// Spawn `sh -c {command}`, wait up to `timeout_secs` seconds (capped at 60), return formatted output.
pub(super) async fn spawn_command(command: &str, timeout_secs: u64) -> String {
    if let Ok(fixture_path) = std::env::var("SIM_FIXTURE") {
        return simulate_command(command, &fixture_path);
    }

    let effective_timeout = timeout_secs.min(60);

    let child = match Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return format!("Failed to spawn command: {}", e),
    };

    match timeout(
        Duration::from_secs(effective_timeout),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.success() {
                if stdout.trim().is_empty() && stderr.trim().is_empty() {
                    "(Command executed successfully, no output)".to_string()
                } else {
                    format!("{}{}", stdout, stderr)
                }
            } else {
                format!(
                    "Exit code: {}\n{}{}",
                    output.status.code().unwrap_or(-1),
                    stdout,
                    stderr
                )
            }
        }
        Ok(Err(e)) => format!("Failed to execute command: {}", e),
        Err(_) => format!("Command timed out after {} seconds", effective_timeout),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use tempfile::NamedTempFile;

    use super::*;

    fn write_fixture(json: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("tempfile");
        write!(f, "{json}").expect("write fixture");
        f
    }

    #[test]
    fn simulate_exact_pattern_match() {
        let f = write_fixture(
            r#"{"command_responses":[{"pattern":"docker ps","output":"CONTAINER abc123"}],"fallback_output":"no match"}"#,
        );
        let out = simulate_command("docker ps", f.path().to_str().unwrap());
        assert_eq!(out, "CONTAINER abc123");
    }

    #[test]
    fn simulate_regex_wildcard_match() {
        let f = write_fixture(
            r#"{"command_responses":[{"pattern":"docker logs .*","output":"ERROR timeout"}],"fallback_output":"no match"}"#,
        );
        let out = simulate_command(
            "docker logs api-service --tail 100",
            f.path().to_str().unwrap(),
        );
        assert_eq!(out, "ERROR timeout");
    }

    #[test]
    fn simulate_fallback_on_no_match() {
        let f = write_fixture(r#"{"command_responses":[],"fallback_output":"custom fallback"}"#);
        let out = simulate_command("unknown command xyz", f.path().to_str().unwrap());
        assert_eq!(out, "custom fallback");
    }

    #[test]
    fn simulate_default_fallback_when_field_absent() {
        let f = write_fixture(r#"{"command_responses":[]}"#);
        let out = simulate_command("xyz", f.path().to_str().unwrap());
        assert_eq!(out, "(SIM) Command not matched in fixture");
    }

    #[test]
    fn simulate_invalid_fixture_path_returns_error_string() {
        let out = simulate_command("docker ps", "/nonexistent/path/fixture.json");
        assert!(out.starts_with("SIM: failed to read fixture"), "got: {out}");
    }

    #[test]
    fn simulate_invalid_json_returns_error_string() {
        let f = write_fixture("not json at all");
        let out = simulate_command("docker ps", f.path().to_str().unwrap());
        assert!(out.starts_with("SIM: invalid fixture JSON"), "got: {out}");
    }

    #[test]
    fn simulate_fixture_allows_description_field() {
        let f = write_fixture(
            r#"{"description":"demo","command_responses":[{"pattern":"docker ps","output":"ok"}],"fallback_output":"no match"}"#,
        );
        let out = simulate_command("docker ps", f.path().to_str().unwrap());
        assert_eq!(out, "ok");
    }
}
