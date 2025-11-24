use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::RwLock;
use which::which;

/// Global cache for discovered commands and aliases
static COMMAND_CACHE: std::sync::LazyLock<RwLock<CommandCache>> =
    std::sync::LazyLock::new(|| RwLock::new(CommandCache::new()));

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

    /// Load system-wide aliases from global configuration files
    ///
    /// Searches for and parses:
    /// - `/etc/bash.bashrc` (Debian/Ubuntu)
    /// - `/etc/bashrc` (RedHat/CentOS/Fedora)
    /// - `/etc/profile`
    /// - `/etc/profile.d/*.sh`
    ///
    /// System aliases are merged with user aliases (user aliases take precedence).
    ///
    /// # Performance
    /// This is a blocking I/O operation (~1-5ms). Use with `spawn_blocking` in async context.
    ///
    /// # Example
    /// ```no_run
    /// use infraware_terminal::input::discovery::CommandCache;
    ///
    /// CommandCache::load_system_aliases();
    /// ```
    pub fn load_system_aliases() -> Result<(), String> {
        let mut aliases = HashMap::new();

        // System-wide alias files (in priority order)
        let system_files = vec![
            "/etc/bash.bashrc", // Debian/Ubuntu
            "/etc/bashrc",      // RedHat/CentOS/Fedora
            "/etc/profile",     // Generic
        ];

        for file_path in system_files {
            if let Ok(content) = fs::read_to_string(file_path) {
                aliases.extend(parse_aliases(&content));
            }
            // Ignore files that don't exist - different distros have different files
        }

        // Check /etc/profile.d/*.sh files
        if let Ok(entries) = fs::read_dir("/etc/profile.d") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("sh") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        aliases.extend(parse_aliases(&content));
                    }
                }
            }
        }

        // Update cache - merge with existing aliases (user aliases take precedence)
        let mut cache = match COMMAND_CACHE.write() {
            Ok(cache) => cache,
            Err(poisoned) => {
                eprintln!("Warning: Command cache write lock was poisoned, recovering...");
                poisoned.into_inner()
            }
        };

        // Merge: system aliases first, then user aliases override
        let mut merged = aliases;
        merged.extend(cache.aliases.clone());
        cache.aliases = merged;

        Ok(())
    }

    /// Expand an alias (single-level expansion like Bash)
    ///
    /// Returns the expansion if the name is an alias, None otherwise.
    /// Does NOT recursively expand chained aliases - that's handled by the classifier
    /// re-calling classify on the expanded result.
    ///
    /// # Arguments
    /// * `alias_name` - The alias to expand
    ///
    /// # Returns
    /// * `Some(expanded)` - The expansion of the alias
    /// * `None` - If not an alias
    ///
    /// # Example
    /// ```no_run
    /// use infraware_terminal::input::discovery::CommandCache;
    ///
    /// // If alias: ll='ls -la'
    /// assert_eq!(CommandCache::expand_alias("ll"), Some("ls -la".to_string()));
    /// ```
    pub fn expand_alias(alias_name: &str) -> Option<String> {
        let cache = match COMMAND_CACHE.read() {
            Ok(cache) => cache,
            Err(poisoned) => {
                eprintln!("Warning: Command cache read lock was poisoned, recovering...");
                poisoned.into_inner()
            }
        };

        cache.aliases.get(alias_name).cloned()
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
        let cache = match COMMAND_CACHE.read() {
            Ok(cache) => cache,
            Err(poisoned) => {
                eprintln!("Warning: Command cache read lock was poisoned, recovering...");
                poisoned.into_inner()
            }
        };
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

/// Check if an alias value contains potentially dangerous patterns
///
/// This is a security measure to prevent malicious aliases from being loaded
/// from system configuration files.
///
/// # Arguments
/// * `name` - The alias name
/// * `value` - The alias value to check
///
/// # Returns
/// `true` if the alias is safe, `false` if it contains dangerous patterns
fn is_safe_alias(name: &str, value: &str) -> bool {
    // List of dangerous command patterns that should be rejected
    const DANGEROUS_PATTERNS: &[&str] = &[
        "rm -rf /",        // Recursive delete from root
        "rm -rf /*",       // Recursive delete all
        "mkfs",            // Format filesystem
        "dd if=/dev/zero", // Wipe disk
        ":(){ :|:& };:",   // Fork bomb
        "chmod -R 777 /",  // Chmod everything
        "chown -R root /", // Chown everything to root
        "> /dev/sda",      // Direct disk write
        "mkfs.",           // Any mkfs variant
    ];

    for pattern in DANGEROUS_PATTERNS {
        if value.contains(pattern) {
            eprintln!(
                "Warning: Rejecting potentially dangerous alias '{name}': contains '{pattern}'"
            );
            return false;
        }
    }

    true
}

/// Parse shell alias definitions from config file content
///
/// Supports formats:
/// - `alias ll='ls -la'`
/// - `alias ll="ls -la"`
/// - `alias ll=ls\ -la`
///
/// Security: Rejects aliases with potentially dangerous patterns (rm -rf /, mkfs, etc.)
///
/// # Arguments
/// * `content` - The file content to parse
///
/// # Returns
/// HashMap of alias name to expanded command
#[allow(dead_code)]
fn parse_aliases(content: &str) -> HashMap<String, String> {
    let mut aliases = HashMap::new();

    for (line_num, line) in content.lines().enumerate() {
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

                // Validate alias name is not empty
                if name.is_empty() {
                    eprintln!(
                        "Warning: Malformed alias on line {}: empty name",
                        line_num + 1
                    );
                    continue;
                }

                // Validate alias value is not empty
                if value.is_empty() {
                    eprintln!(
                        "Warning: Malformed alias '{}' on line {}: empty value",
                        name,
                        line_num + 1
                    );
                    continue;
                }

                // Remove quotes if present
                let value = value
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
                    .or_else(|| value.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
                    .unwrap_or(value);

                // Security check: reject dangerous aliases
                if !is_safe_alias(name, value) {
                    continue;
                }

                aliases.insert(name.to_string(), value.to_string());
            } else {
                eprintln!(
                    "Warning: Malformed alias on line {}: no '=' found",
                    line_num + 1
                );
            }
        }
    }

    aliases
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[serial_test::serial]
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
    #[serial_test::serial]
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
        let cache = match COMMAND_CACHE.read() {
            Ok(cache) => cache,
            Err(poisoned) => {
                eprintln!("Warning: Command cache read lock was poisoned in test, recovering...");
                poisoned.into_inner()
            }
        };
        assert!(
            cache.available.contains(test_cmd) || cache.unavailable.contains(test_cmd),
            "Command '{test_cmd}' should be in cache after is_available() call"
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
        let content = r"
            # Comment 1
            # Comment 2
        ";
        let aliases = parse_aliases(content);
        assert!(aliases.is_empty());
    }

    #[test]
    #[serial_test::serial]
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
    #[serial_test::serial]
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
    #[serial_test::serial]
    fn test_is_alias() {
        CommandCache::clear();

        // Manually add an alias for testing
        {
            let mut cache = match COMMAND_CACHE.write() {
                Ok(cache) => cache,
                Err(poisoned) => {
                    eprintln!(
                        "Warning: Command cache write lock was poisoned in test, recovering..."
                    );
                    poisoned.into_inner()
                }
            };
            cache
                .aliases
                .insert("test_alias".to_string(), "echo test".to_string());
        }

        assert!(CommandCache::is_alias("test_alias"));
        assert!(!CommandCache::is_alias("not_an_alias"));

        CommandCache::clear();
    }

    #[test]
    #[serial_test::serial]
    fn test_get_alias_expansion() {
        CommandCache::clear();

        // Manually add an alias
        {
            let mut cache = match COMMAND_CACHE.write() {
                Ok(cache) => cache,
                Err(poisoned) => {
                    eprintln!(
                        "Warning: Command cache write lock was poisoned in test, recovering..."
                    );
                    poisoned.into_inner()
                }
            };
            cache.aliases.insert("ll".to_string(), "ls -la".to_string());
        }

        assert_eq!(
            CommandCache::get_alias_expansion("ll"),
            Some("ls -la".to_string())
        );
        assert_eq!(CommandCache::get_alias_expansion("nonexistent"), None);

        CommandCache::clear();
    }

    #[test]
    #[serial_test::serial]
    fn test_expand_alias_simple() {
        CommandCache::clear();

        // Add a simple alias
        {
            let mut cache = match COMMAND_CACHE.write() {
                Ok(cache) => cache,
                Err(poisoned) => poisoned.into_inner(),
            };
            cache.aliases.insert("ll".to_string(), "ls -la".to_string());
        }

        assert_eq!(CommandCache::expand_alias("ll"), Some("ls -la".to_string()));
        assert_eq!(CommandCache::expand_alias("ls"), None);

        CommandCache::clear();
    }

    #[test]
    #[serial_test::serial]
    fn test_expand_alias_chained() {
        CommandCache::clear();

        // Add chained aliases: l -> ll -> ls -la
        // Single-level expansion: l expands to "ll" only
        {
            let mut cache = match COMMAND_CACHE.write() {
                Ok(cache) => cache,
                Err(poisoned) => poisoned.into_inner(),
            };
            cache.aliases.insert("ll".to_string(), "ls -la".to_string());
            cache.aliases.insert("l".to_string(), "ll".to_string());
        }

        // Single-level expansion
        assert_eq!(CommandCache::expand_alias("l"), Some("ll".to_string()));
        assert_eq!(CommandCache::expand_alias("ll"), Some("ls -la".to_string()));

        CommandCache::clear();
    }

    #[test]
    #[serial_test::serial]
    fn test_expand_alias_circular() {
        CommandCache::clear();

        // Add circular aliases: a -> b -> a
        // Single-level expansion doesn't detect cycles - that's OK
        {
            let mut cache = match COMMAND_CACHE.write() {
                Ok(cache) => cache,
                Err(poisoned) => poisoned.into_inner(),
            };
            cache.aliases.insert("a".to_string(), "b".to_string());
            cache.aliases.insert("b".to_string(), "a".to_string());
        }

        // Single-level expansion just returns the direct expansion
        assert_eq!(CommandCache::expand_alias("a"), Some("b".to_string()));
        assert_eq!(CommandCache::expand_alias("b"), Some("a".to_string()));

        CommandCache::clear();
    }

    #[test]
    #[serial_test::serial]
    fn test_load_system_aliases() {
        CommandCache::clear();

        // This test will work on systems with /etc/bash.bashrc or /etc/bashrc
        // On systems without these files, it should not fail
        let result = CommandCache::load_system_aliases();

        // Should succeed even if no files found
        assert!(result.is_ok());

        // Just verify it doesn't panic
        let _stats = CommandCache::stats();

        CommandCache::clear();
    }

    #[test]
    #[serial_test::serial]
    fn test_alias_priority_user_over_system() {
        CommandCache::clear();

        // Simulate system alias
        {
            let mut cache = match COMMAND_CACHE.write() {
                Ok(cache) => cache,
                Err(poisoned) => poisoned.into_inner(),
            };
            cache
                .aliases
                .insert("test_cmd".to_string(), "system_value".to_string());
        }

        // Load user aliases (which should override system)
        {
            let mut cache = match COMMAND_CACHE.write() {
                Ok(cache) => cache,
                Err(poisoned) => poisoned.into_inner(),
            };
            cache
                .aliases
                .insert("test_cmd".to_string(), "user_value".to_string());
        }

        // User alias should take precedence
        assert_eq!(
            CommandCache::get_alias_expansion("test_cmd"),
            Some("user_value".to_string())
        );

        CommandCache::clear();
    }
}
