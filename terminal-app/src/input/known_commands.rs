/// Canonical list of known DevOps and shell commands
///
/// This module provides the single source of truth for known commands used by:
/// - `KnownCommandHandler`: For command classification
/// - `TypoDetectionHandler`: For typo detection and suggestions
///
/// Maintaining a single list ensures consistency and reduces maintenance burden.

/// Returns the default list of known DevOps and shell commands
///
/// # Categories
/// - Basic shell commands (ls, cd, pwd, cat, etc.)
/// - Text processing tools (sed, awk, grep, etc.)
/// - Process management (ps, top, kill, etc.)
/// - Network utilities (curl, wget, ssh, etc.)
/// - System information (uname, df, free, etc.)
/// - Docker and container tools
/// - Kubernetes and orchestration
/// - Cloud provider CLIs (AWS, Azure, GCP)
/// - Version control (git, svn, hg)
/// - Build tools (cargo, npm, make, etc.)
/// - Infrastructure as Code (terraform, ansible, etc.)
///
/// # Example
/// ```
/// use infraware_terminal::input::known_commands::default_devops_commands;
///
/// let commands = default_devops_commands();
/// assert!(commands.contains(&"docker".to_string()));
/// assert!(commands.contains(&"kubectl".to_string()));
/// ```
pub fn default_devops_commands() -> Vec<String> {
    vec![
        // Basic shell
        "ls",
        "cd",
        "pwd",
        "cat",
        "echo",
        "grep",
        "find",
        "mkdir",
        "rm",
        "cp",
        "mv",
        "touch",
        "chmod",
        "chown",
        "ln",
        "tar",
        "gzip",
        "gunzip",
        "zip",
        "unzip",
        // Text processing
        "sed",
        "awk",
        "sort",
        "uniq",
        "wc",
        "head",
        "tail",
        "cut",
        "paste",
        "tr",
        // Process management
        "ps",
        "kill",
        "killall",
        "pkill",
        "jobs",
        "bg",
        "fg",
        // Network
        "curl",
        "wget",
        "ping",
        "netstat",
        "ss",
        "ip",
        "ifconfig",
        "dig",
        "nslookup",
        "traceroute",
        "ssh",
        "scp",
        "rsync",
        // System info
        "uname",
        "hostname",
        "whoami",
        "who",
        "w",
        "uptime",
        "free",
        "df",
        "du",
        // Docker
        "docker",
        "docker-compose",
        "docker-machine",
        // Kubernetes
        "kubectl",
        "helm",
        "minikube",
        "k9s",
        // Cloud providers
        "aws",
        "az",
        "gcloud",
        "terraform",
        "terragrunt",
        "pulumi",
        // Version control
        "git",
        "svn",
        "hg",
        // Build tools
        "make",
        "cmake",
        "cargo",
        "npm",
        "yarn",
        "pip",
        "pipenv",
        "poetry",
        "maven",
        "gradle",
        "ant",
        // Monitoring
        "prometheus",
        "grafana",
        "datadog",
        // Other DevOps tools
        "ansible",
        "ansible-playbook",
        "vagrant",
        "packer",
        "consul",
        "vault",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_commands_not_empty() {
        let commands = default_devops_commands();
        assert!(!commands.is_empty());
        assert!(commands.len() > 50, "Should have 50+ commands");
    }

    #[test]
    fn test_contains_docker_commands() {
        let commands = default_devops_commands();
        assert!(commands.contains(&"docker".to_string()));
        assert!(commands.contains(&"docker-compose".to_string()));
    }

    #[test]
    fn test_contains_kubernetes_commands() {
        let commands = default_devops_commands();
        assert!(commands.contains(&"kubectl".to_string()));
        assert!(commands.contains(&"helm".to_string()));
    }

    #[test]
    fn test_contains_basic_shell_commands() {
        let commands = default_devops_commands();
        assert!(commands.contains(&"ls".to_string()));
        assert!(commands.contains(&"cd".to_string()));
        assert!(commands.contains(&"pwd".to_string()));
    }

    #[test]
    fn test_no_duplicates() {
        let commands = default_devops_commands();
        let mut sorted = commands.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            commands.len(),
            sorted.len(),
            "Command list should not contain duplicates"
        );
    }
}
