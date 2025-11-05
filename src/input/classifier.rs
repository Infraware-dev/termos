/// Input classification: Command vs Natural Language
use anyhow::Result;

/// Represents the type of user input
#[derive(Debug, Clone, PartialEq)]
pub enum InputType {
    /// A shell command with its name and arguments
    Command(String, Vec<String>),
    /// Natural language query or phrase
    NaturalLanguage(String),
    /// Empty input
    Empty,
}

/// Classifier for determining if input is a command or natural language
pub struct InputClassifier {
    known_commands: Vec<String>,
}

impl InputClassifier {
    /// Create a new input classifier with default known commands
    pub fn new() -> Self {
        Self {
            known_commands: Self::default_known_commands(),
        }
    }

    /// Default list of known DevOps and shell commands
    fn default_known_commands() -> Vec<String> {
        vec![
            // Basic shell
            "ls", "cd", "pwd", "cat", "echo", "grep", "find", "mkdir", "rm", "cp", "mv",
            "touch", "chmod", "chown", "ln", "tar", "gzip", "gunzip", "zip", "unzip",
            // Text processing
            "sed", "awk", "sort", "uniq", "wc", "head", "tail", "cut", "paste", "tr",
            // Process management
            "ps", "top", "htop", "kill", "killall", "pkill", "jobs", "bg", "fg",
            // Network
            "curl", "wget", "ping", "netstat", "ss", "ip", "ifconfig", "dig", "nslookup",
            "traceroute", "ssh", "scp", "rsync",
            // System info
            "uname", "hostname", "whoami", "who", "w", "uptime", "free", "df", "du",
            // Docker
            "docker", "docker-compose", "docker-machine",
            // Kubernetes
            "kubectl", "helm", "minikube", "k9s",
            // Cloud providers
            "aws", "az", "gcloud", "terraform", "terragrunt", "pulumi",
            // Version control
            "git", "svn", "hg",
            // Build tools
            "make", "cmake", "cargo", "npm", "yarn", "pip", "pipenv", "poetry",
            "maven", "gradle", "ant",
            // Monitoring
            "prometheus", "grafana", "datadog",
            // Other DevOps tools
            "ansible", "ansible-playbook", "vagrant", "packer", "consul", "vault",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }

    /// Classify the input as command or natural language
    pub fn classify(&self, input: &str) -> Result<InputType> {
        let trimmed = input.trim();

        // Empty input
        if trimmed.is_empty() {
            return Ok(InputType::Empty);
        }

        // 1. Check against known commands whitelist
        if self.is_known_command(trimmed) {
            return Ok(self.parse_as_command(trimmed)?);
        }

        // 2. Check if looks like a command (before natural language heuristics)
        if self.looks_like_command(trimmed) {
            return Ok(self.parse_as_command(trimmed)?);
        }

        // 3. Heuristics for natural language detection
        if self.is_likely_natural_language(trimmed) {
            return Ok(InputType::NaturalLanguage(trimmed.to_string()));
        }

        // 4. Default: treat as natural language
        Ok(InputType::NaturalLanguage(trimmed.to_string()))
    }

    /// Check if the input starts with a known command
    fn is_known_command(&self, input: &str) -> bool {
        let first_word = input.split_whitespace().next().unwrap_or("");
        self.known_commands.iter().any(|cmd| cmd == first_word)
    }

    /// Check if input looks like a command based on syntax
    fn looks_like_command(&self, input: &str) -> bool {
        // Contains flags
        if input.contains(" -") || input.contains(" --") {
            return true;
        }

        // Contains pipes or redirects
        if input.contains('|') || input.contains('>') || input.contains('<') {
            return true;
        }

        // Environment variable syntax
        if input.contains("$") || input.contains("${") {
            return true;
        }

        // Looks like a path
        if input.starts_with('/') || input.starts_with("./") || input.starts_with("../") {
            return true;
        }

        // Single word without spaces (might be a command)
        if !input.contains(' ') && input.len() < 20 {
            return true;
        }

        false
    }

    /// Check if input is likely natural language
    fn is_likely_natural_language(&self, input: &str) -> bool {
        let lowercase = input.to_lowercase();

        // Question words at the start
        let question_words = ["how", "what", "why", "when", "where", "who", "can you", "could you"];
        for word in &question_words {
            if lowercase.starts_with(word) {
                return true;
            }
        }

        // Contains question marks
        if input.contains('?') {
            return true;
        }

        // Contains articles
        let articles = [" a ", " an ", " the "];
        for article in &articles {
            if lowercase.contains(article) {
                return true;
            }
        }

        // Common natural language verbs
        let nl_verbs = ["show me", "explain", "help", "tell me", "describe"];
        for verb in &nl_verbs {
            if lowercase.contains(verb) {
                return true;
            }
        }

        // Long input with multiple words (likely natural language)
        let word_count = input.split_whitespace().count();
        if word_count > 5 && !self.looks_like_command(input) {
            return true;
        }

        // Contains common punctuation
        if input.contains(',') || input.contains('.') {
            return true;
        }

        false
    }

    /// Parse input as a command
    fn parse_as_command(&self, input: &str) -> Result<InputType> {
        let parts = shell_words::split(input)?;

        if parts.is_empty() {
            return Ok(InputType::Empty);
        }

        Ok(InputType::Command(
            parts[0].clone(),
            parts[1..].to_vec(),
        ))
    }

    /// Add a command to the known commands list
    pub fn add_known_command(&mut self, command: String) {
        if !self.known_commands.contains(&command) {
            self.known_commands.push(command);
        }
    }
}

impl Default for InputClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_commands() {
        let classifier = InputClassifier::new();

        // Basic commands
        assert!(matches!(
            classifier.classify("ls -la").unwrap(),
            InputType::Command(_, _)
        ));
        assert!(matches!(
            classifier.classify("docker ps").unwrap(),
            InputType::Command(_, _)
        ));
        assert!(matches!(
            classifier.classify("kubectl get pods").unwrap(),
            InputType::Command(_, _)
        ));
    }

    #[test]
    fn test_natural_language() {
        let classifier = InputClassifier::new();

        assert!(matches!(
            classifier.classify("how do I list files?").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("what is kubernetes").unwrap(),
            InputType::NaturalLanguage(_)
        ));
        assert!(matches!(
            classifier.classify("show me the logs").unwrap(),
            InputType::NaturalLanguage(_)
        ));
    }

    #[test]
    fn test_command_syntax() {
        let classifier = InputClassifier::new();

        // Flags
        assert!(matches!(
            classifier.classify("unknown-cmd --flag").unwrap(),
            InputType::Command(_, _)
        ));

        // Pipes
        assert!(matches!(
            classifier.classify("cat file.txt | grep pattern").unwrap(),
            InputType::Command(_, _)
        ));
    }
}
