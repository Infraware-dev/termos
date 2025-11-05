/// Tab completion for commands and file paths
use std::env;
use std::fs;
use std::path::PathBuf;

/// Tab completion handler
pub struct TabCompletion;

impl TabCompletion {
    /// Get completions for the given partial input
    pub fn get_completions(partial: &str) -> Vec<String> {
        // If no space, complete commands
        if !partial.contains(' ') {
            Self::complete_command(partial)
        } else {
            // Complete file paths
            Self::complete_file_path(partial)
        }
    }

    /// Complete command names
    fn complete_command(partial: &str) -> Vec<String> {
        let mut completions = Vec::new();

        // Get executables from PATH
        if let Ok(path_var) = env::var("PATH") {
            for path_dir in env::split_paths(&path_var) {
                if let Ok(entries) = fs::read_dir(path_dir) {
                    for entry in entries.flatten() {
                        if let Some(name) = entry.file_name().to_str() {
                            if name.starts_with(partial) {
                                // Check if it's executable
                                #[cfg(unix)]
                                {
                                    use std::os::unix::fs::PermissionsExt;
                                    if let Ok(metadata) = entry.metadata() {
                                        let permissions = metadata.permissions();
                                        if permissions.mode() & 0o111 != 0 {
                                            completions.push(name.to_string());
                                        }
                                    }
                                }
                                #[cfg(not(unix))]
                                {
                                    completions.push(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        completions.sort();
        completions.dedup();
        completions
    }

    /// Complete file paths
    fn complete_file_path(partial: &str) -> Vec<String> {
        // Extract the path part after the last space
        let parts: Vec<&str> = partial.rsplitn(2, ' ').collect();
        if parts.is_empty() {
            return Vec::new();
        }

        let path_part = parts[0];
        let prefix_part = if parts.len() > 1 { parts[1] } else { "" };

        // Split into directory and file prefix
        let (dir, file_prefix) = if path_part.contains('/') {
            let idx = path_part.rfind('/').unwrap();
            (&path_part[..=idx], &path_part[idx + 1..])
        } else {
            (".", path_part)
        };

        // Expand ~ to home directory
        let dir_path = if dir.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                PathBuf::from(dir.replace('~', home.to_str().unwrap_or("~")))
            } else {
                PathBuf::from(dir)
            }
        } else {
            PathBuf::from(dir)
        };

        let mut results = Vec::new();

        if let Ok(entries) = fs::read_dir(dir_path) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(file_prefix) && !name.starts_with('.') {
                        // Reconstruct the full completion
                        let completion = if !prefix_part.is_empty() {
                            if dir == "." {
                                format!("{} {}", prefix_part, name)
                            } else {
                                format!("{} {}{}", prefix_part, dir, name)
                            }
                        } else if dir == "." {
                            name.to_string()
                        } else {
                            format!("{}{}", dir, name)
                        };

                        // Add trailing slash for directories
                        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                            results.push(format!("{}/", completion));
                        } else {
                            results.push(completion);
                        }
                    }
                }
            }
        }

        results.sort();
        results
    }

    /// Get the common prefix of all completions
    pub fn get_common_prefix(completions: &[String]) -> String {
        if completions.is_empty() {
            return String::new();
        }

        if completions.len() == 1 {
            return completions[0].clone();
        }

        let mut prefix = completions[0].clone();
        for completion in &completions[1..] {
            while !completion.starts_with(&prefix) {
                prefix.pop();
                if prefix.is_empty() {
                    return String::new();
                }
            }
        }

        prefix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_command() {
        let completions = TabCompletion::complete_command("ec");
        // Should include echo on most systems
        assert!(!completions.is_empty());
    }

    #[test]
    fn test_get_common_prefix() {
        let completions = vec![
            "hello.txt".to_string(),
            "hello.md".to_string(),
            "hello.rs".to_string(),
        ];
        let prefix = TabCompletion::get_common_prefix(&completions);
        assert_eq!(prefix, "hello.");
    }

    #[test]
    fn test_get_common_prefix_single() {
        let completions = vec!["single.txt".to_string()];
        let prefix = TabCompletion::get_common_prefix(&completions);
        assert_eq!(prefix, "single.txt");
    }

    #[test]
    fn test_get_common_prefix_empty() {
        let completions: Vec<String> = vec![];
        let prefix = TabCompletion::get_common_prefix(&completions);
        assert_eq!(prefix, "");
    }
}
