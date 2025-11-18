/// Command discovery and caching for SCAN algorithm
///
/// This module provides PATH-aware command discovery and user alias loading
/// with thread-safe caching for optimal performance.
///
/// # Design Patterns
/// - **Lazy Singleton**: Global command cache initialized on first access
/// - **Cache-Aside**: Check cache before expensive PATH/alias lookups
/// - **RwLock**: Thread-safe concurrent read access (99% read workload)
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::RwLock;
use which::which;

/// Global cache for discovered commands and aliases
static COMMAND_CACHE: Lazy<RwLock<CommandCache>> = Lazy::new(|| RwLock::new(CommandCache::new()));

/// Cache for command availability and user aliases
///
/// Maintains two sets for fast lookups:
/// - `available`: Commands that exist in PATH
/// - `unavailable`: Commands that have been checked and don't exist
/// - `aliases`: User-defined command aliases from shell config files
pub struct CommandCache {
    available: HashSet<String>,
    unavailable: HashSet<String>,
    aliases: HashMap<String, String>,
}

impl CommandCache {
    /// Create a new empty command cache
    fn new() -> Self {
        Self {
            available: HashSet::new(),
            unavailable: HashSet::new(),
            aliases: HashMap::new(),
        }
    }

    /// Check if a command is available in PATH (with caching)
    ///
    /// # Performance
    /// - Cache hit: O(1) hash lookup
    /// - Cache miss: O(PATH_length) via `which` crate
    ///
    /// # Example
    /// ```
    /// use infraware_terminal::input::discovery::CommandCache;
    ///
    /// assert!(CommandCache::is_available("ls"));
    /// assert!(!CommandCache::is_available("nonexistent-cmd-12345"));
    /// ```
    pub fn is_available(command: &str) -> bool {
        // Fast path: check cache first (read lock)
        {
            let cache = match COMMAND_CACHE.read() {
                Ok(cache) => cache,
                Err(poisoned) => {
                    // Lock was poisoned, but we can still access the data
                    eprintln!("Warning: Command cache read lock was poisoned, recovering...");
                    poisoned.into_inner()
                }
            };

            if cache.available.contains(command) {
                return true;
            }
            if cache.unavailable.contains(command) {
                return false;
            }
        }

        // Slow path: check using `which` crate
        let exists = which(command).is_ok();

        // Update cache (write lock)
        {
            let mut cache = match COMMAND_CACHE.write() {
                Ok(cache) => cache,
                Err(poisoned) => {
                    eprintln!("Warning: Command cache write lock was poisoned, recovering...");
                    poisoned.into_inner()
                }
            };

            if exists {
                cache.available.insert(command.to_string());
            } else {
                cache.unavailable.insert(command.to_string());
            }
        }

        exists
    }

    /// Check if a command is a user-defined alias
    ///
    /// # Example
    /// ```
    /// use infraware_terminal::input::discovery::CommandCache;
    ///
    /// // After loading aliases from shell config
    /// CommandCache::load_user_aliases();
    /// // Check if alias exists
    /// let is_alias = CommandCache::is_alias("ll");
    /// ```
    #[allow(dead_code)]
    pub fn is_alias(command: &str) -> bool {
        let cache = match COMMAND_CACHE.read() {
            Ok(cache) => cache,
            Err(poisoned) => {
                eprintln!("Warning: Command cache read lock was poisoned, recovering...");
                poisoned.into_inner()
            }
        };
        cache.aliases.contains_key(command)
    }

    /// Get the expanded command for an alias
    ///
    /// Returns `None` if not an alias, or `Some(expanded_command)` if found.
    #[allow(dead_code)]
    pub fn get_alias_expansion(alias: &str) -> Option<String> {
        let cache = match COMMAND_CACHE.read() {
            Ok(cache) => cache,
            Err(poisoned) => {
                eprintln!("Warning: Command cache read lock was poisoned, recovering...");
                poisoned.into_inner()
            }
        };
        cache.aliases.get(alias).cloned()
    }

    /// Load user aliases from shell configuration files
    ///
    /// Searches for and parses:
    /// - `~/.bashrc`
    /// - `~/.bash_aliases`
    /// - `~/.zshrc`
    ///
    /// Aliases are cached for fast lookup.
    ///
    /// # Example
    /// ```no_run
    /// use infraware_terminal::input::discovery::CommandCache;
    ///
    /// CommandCache::load_user_aliases();
    /// ```
    #[allow(dead_code)]
    pub fn load_user_aliases() {
        let mut aliases = HashMap::new();

        if let Some(home) = dirs::home_dir() {
            // Check bash aliases
            let bash_aliases = home.join(".bash_aliases");
            if bash_aliases.exists() {
                if let Ok(content) = fs::read_to_string(&bash_aliases) {
                    aliases.extend(parse_aliases(&content));
                }
            }

            // Check .bashrc
            let bashrc = home.join(".bashrc");
            if bashrc.exists() {
                if let Ok(content) = fs::read_to_string(&bashrc) {
                    aliases.extend(parse_aliases(&content));
                }
            }

            // Check .zshrc
            let zshrc = home.join(".zshrc");
            if zshrc.exists() {
                if let Ok(content) = fs::read_to_string(&zshrc) {
                    aliases.extend(parse_aliases(&content));
                }
            }
        }

        // Update cache
        let mut cache = match COMMAND_CACHE.write() {
            Ok(cache) => cache,
            Err(poisoned) => {
                eprintln!("Warning: Command cache write lock was poisoned, recovering...");
                poisoned.into_inner()
            }
        };
        cache.aliases = aliases;
    }

    /// Clear the entire cache (useful for testing or when PATH changes)
    ///
    /// # Example
    /// ```
    /// use infraware_terminal::input::discovery::CommandCache;
    ///
    /// CommandCache::clear();
    /// ```
    #[allow(dead_code)]
    pub fn clear() {
        let mut cache = match COMMAND_CACHE.write() {
            Ok(cache) => cache,
            Err(poisoned) => {
                eprintln!("Warning: Command cache write lock was poisoned, recovering...");
                poisoned.into_inner()
            }
        };
        cache.available.clear();
        cache.unavailable.clear();
        cache.aliases.clear();
    }

    /// Get statistics about cache contents (for debugging/monitoring)
    #[allow(dead_code)]
    pub fn stats() -> CacheStats {
        let cache = COMMAND_CACHE.read().unwrap();
        CacheStats {
            available_count: cache.available.len(),
            unavailable_count: cache.unavailable.len(),
            alias_count: cache.aliases.len(),
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CacheStats {
    pub available_count: usize,
    pub unavailable_count: usize,
    pub alias_count: usize,
}

/// Parse shell alias definitions from config file content
///
/// Supports formats:
/// - `alias ll='ls -la'`
/// - `alias ll="ls -la"`
/// - `alias ll=ls\ -la`
///
/// # Arguments
/// * `content` - The file content to parse
///
/// # Returns
/// HashMap of alias name to expanded command
#[allow(dead_code)]
fn parse_aliases(content: &str) -> HashMap<String, String> {
    let mut aliases = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Look for alias definitions: alias name='command'
        if let Some(alias_def) = trimmed.strip_prefix("alias ") {
            if let Some(eq_pos) = alias_def.find('=') {
                let name = alias_def[..eq_pos].trim();
                let value = alias_def[eq_pos + 1..].trim();

                // Remove quotes if present
                let value = value
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
                    .or_else(|| value.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
                    .unwrap_or(value);

                aliases.insert(name.to_string(), value.to_string());
            }
        }
    }

    aliases
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_availability() {
        // Clear cache before testing
        CommandCache::clear();

        // Use platform-specific commands that definitely exist
        #[cfg(unix)]
        let existing_cmd = "sh"; // sh always exists on Unix
        #[cfg(windows)]
        let existing_cmd = "cmd"; // cmd.exe always exists on Windows

        assert!(CommandCache::is_available(existing_cmd));

        // Command that definitely doesn't exist
        assert!(!CommandCache::is_available("nonexistent-cmd-12345-xyz"));

        // Check cache is populated
        let stats = CommandCache::stats();
        assert!(stats.available_count > 0 || stats.unavailable_count > 0);
    }

    #[test]
    fn test_command_caching() {
        CommandCache::clear();

        // Use a command that definitely exists on all platforms
        #[cfg(unix)]
        let test_cmd = "sh"; // sh always exists on Unix systems
        #[cfg(windows)]
        let test_cmd = "cmd"; // cmd.exe always exists on Windows

        // First call - cache miss
        let exists = CommandCache::is_available(test_cmd);

        // Second call - should be cache hit
        let exists_cached = CommandCache::is_available(test_cmd);

        assert_eq!(exists, exists_cached);

        // Verify it's in cache (should be in available since sh/cmd always exist)
        let cache = COMMAND_CACHE.read().unwrap();
        assert!(
            cache.available.contains(test_cmd) || cache.unavailable.contains(test_cmd),
            "Command '{}' should be in cache after is_available() call",
            test_cmd
        );
    }

    #[test]
    fn test_parse_aliases() {
        let content = r#"
            # This is a comment
            alias ll='ls -la'
            alias gs="git status"
            alias la=ls\ -a

            # Another comment
            alias dc='docker-compose'
        "#;

        let aliases = parse_aliases(content);

        assert_eq!(aliases.get("ll"), Some(&"ls -la".to_string()));
        assert_eq!(aliases.get("gs"), Some(&"git status".to_string()));
        assert_eq!(aliases.get("dc"), Some(&"docker-compose".to_string()));
        assert_eq!(aliases.get("la"), Some(&"ls\\ -a".to_string()));
        assert_eq!(aliases.len(), 4); // All 4 aliases successfully parsed
    }

    #[test]
    fn test_alias_with_single_quotes() {
        let content = "alias ll='ls -la'";
        let aliases = parse_aliases(content);
        assert_eq!(aliases.get("ll"), Some(&"ls -la".to_string()));
    }

    #[test]
    fn test_alias_with_double_quotes() {
        let content = r#"alias gs="git status""#;
        let aliases = parse_aliases(content);
        assert_eq!(aliases.get("gs"), Some(&"git status".to_string()));
    }

    #[test]
    fn test_alias_without_quotes() {
        let content = "alias k=kubectl";
        let aliases = parse_aliases(content);
        assert_eq!(aliases.get("k"), Some(&"kubectl".to_string()));
    }

    #[test]
    fn test_empty_content() {
        let aliases = parse_aliases("");
        assert!(aliases.is_empty());
    }

    #[test]
    fn test_only_comments() {
        let content = r#"
            # Comment 1
            # Comment 2
        "#;
        let aliases = parse_aliases(content);
        assert!(aliases.is_empty());
    }

    #[test]
    fn test_cache_clear() {
        // Add some entries
        CommandCache::is_available("ls");
        CommandCache::load_user_aliases();

        // Clear
        CommandCache::clear();

        // Verify empty
        let stats = CommandCache::stats();
        assert_eq!(stats.available_count, 0);
        assert_eq!(stats.unavailable_count, 0);
        assert_eq!(stats.alias_count, 0);
    }

    #[test]
    fn test_alias_loading() {
        // Note: This test may not find aliases if user doesn't have them
        // It should not fail, just verify the function runs without errors
        CommandCache::clear();
        CommandCache::load_user_aliases();

        // Just verify it doesn't panic
        let _stats = CommandCache::stats();
        // Test passes if stats() doesn't panic
    }

    #[test]
    fn test_is_alias() {
        CommandCache::clear();

        // Manually add an alias for testing
        {
            let mut cache = COMMAND_CACHE.write().unwrap();
            cache
                .aliases
                .insert("test_alias".to_string(), "echo test".to_string());
        }

        assert!(CommandCache::is_alias("test_alias"));
        assert!(!CommandCache::is_alias("not_an_alias"));

        CommandCache::clear();
    }

    #[test]
    fn test_get_alias_expansion() {
        CommandCache::clear();

        // Manually add an alias
        {
            let mut cache = COMMAND_CACHE.write().unwrap();
            cache.aliases.insert("ll".to_string(), "ls -la".to_string());
        }

        assert_eq!(
            CommandCache::get_alias_expansion("ll"),
            Some("ls -la".to_string())
        );
        assert_eq!(CommandCache::get_alias_expansion("nonexistent"), None);

        CommandCache::clear();
    }
}
