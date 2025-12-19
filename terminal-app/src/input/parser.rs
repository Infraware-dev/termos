/// Command parsing utilities (M2/M3)
use anyhow::Result;

/// Parser for shell commands
#[derive(Debug)]
pub struct CommandParser;

impl CommandParser {
    /// Parse a command string into command and arguments
    pub fn parse(input: &str) -> Result<(String, Vec<String>)> {
        let parts = shell_words::split(input)?;

        if parts.is_empty() {
            anyhow::bail!("Empty command");
        }

        Ok((parts[0].clone(), parts[1..].to_vec()))
    }

    /// Quote an argument if it contains spaces
    pub fn quote_if_needed(arg: &str) -> String {
        if arg.contains(' ') || arg.contains('\t') {
            format!("\"{}\"", arg.replace('"', "\\\""))
        } else {
            arg.to_string()
        }
    }

    /// Join command and arguments back into a string
    pub fn join(command: &str, args: &[String]) -> String {
        let mut result = command.to_string();
        for arg in args {
            result.push(' ');
            result.push_str(&Self::quote_if_needed(arg));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let (cmd, args) = CommandParser::parse("ls -la").unwrap();
        assert_eq!(cmd, "ls");
        assert_eq!(args, vec!["-la"]);
    }

    #[test]
    fn test_parse_with_quotes() {
        let (cmd, args) = CommandParser::parse(r#"echo "hello world""#).unwrap();
        assert_eq!(cmd, "echo");
        assert_eq!(args, vec!["hello world"]);
    }

    #[test]
    fn test_parse_complex() {
        let (cmd, args) = CommandParser::parse(r"docker run -it --name test ubuntu bash").unwrap();
        assert_eq!(cmd, "docker");
        assert_eq!(args, vec!["run", "-it", "--name", "test", "ubuntu", "bash"]);
    }

    #[test]
    fn test_quote_if_needed() {
        assert_eq!(CommandParser::quote_if_needed("simple"), "simple");
        assert_eq!(CommandParser::quote_if_needed("has space"), "\"has space\"");
    }

    #[test]
    fn test_join() {
        let result = CommandParser::join("echo", &["hello".to_string(), "world test".to_string()]);
        assert_eq!(result, r#"echo hello "world test""#);
    }
}
