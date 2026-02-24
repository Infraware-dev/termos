//! Shared shell command execution primitives.
use std::process::Stdio;

use tokio::process::Command;
use tokio::time::{Duration, timeout};

/// Spawn `sh -c {command}`, wait up to `timeout_secs` seconds (capped at 60), return formatted output.
pub(super) async fn spawn_command(command: &str, timeout_secs: u64) -> String {
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

    match timeout(Duration::from_secs(effective_timeout), child.wait_with_output()).await {
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
