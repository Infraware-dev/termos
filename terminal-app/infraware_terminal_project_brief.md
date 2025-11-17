# Infraware Terminal - Project Brief
## Milestone 1 Implementation Guide

**Date**: November 5, 2025  
**Focus**: Terminal Core MVP (Month 1)  
**Tech Stack**: Rust (TUI-based, not full GUI)

---

## Executive Summary

This is NOT a traditional terminal emulator. It's a **hybrid command interpreter** that:
1. Accepts user input
2. Classifies it as either a shell command OR natural language
3. If command: executes it (with auto-install if missing)
4. If natural language: routes to LLM backend
5. Displays results in a TUI interface

**Target use case**: DevOps operations in cloud environments (AWS/Azure) with AI assistance.

---

## Architecture Overview

```
┌─────────────────────────────────────┐
│         TUI Frontend (Rust)         │
│    (ratatui/crossterm based)        │
└──────────────┬──────────────────────┘
               │
    ┌──────────▼──────────┐
    │   Input Classifier   │
    │  (command vs phrase) │
    └──────────┬───────────┘
               │
       ┌───────┴────────┐
       │                │
  ┌────▼─────┐    ┌────▼─────┐
  │ Command  │    │   LLM    │
  │ Executor │    │  Router  │
  └──────────┘    └──────────┘
```

---

## Milestone 1 Breakdown

### M1.1: Core Terminal Display (TUI)

**Technology Choice**: TUI (Text User Interface), NOT heavy GUI
- Lighter and faster
- Native feel for DevOps users
- Cross-platform by default
- Similar to Claude Code interface

**Recommended Stack**:
```toml
[dependencies]
ratatui = "0.29"      # Modern TUI framework
crossterm = "0.28"    # Cross-platform terminal control
tokio = "1.40"        # Async runtime
```

**Core Data Structures**:
```rust
struct Terminal {
    output_buffer: Vec<String>,  // Command/LLM output history
    input_buffer: String,         // Current user input
    mode: TerminalMode,           // Current state
    cursor_position: usize,
}

enum TerminalMode {
    Normal,              // Waiting for input
    ExecutingCommand,    // Running shell command
    WaitingLLM,         // Querying LLM
    PromptingInstall,   // Asking to install missing command
}
```

**Key Tasks**:
- Basic TUI rendering with ratatui
- Input buffer display
- Output scrollback buffer
- Cursor handling
- Keyboard event capture

---

### M1.2: Input Handling & Command Classification

**CRITICAL COMPONENT**: Distinguishing commands from natural language

#### Classification Strategy

```rust
enum InputType {
    Command(String, Vec<String>),  // (command, args)
    NaturalLanguage(String),       // Human phrase/question
    Empty,
}

fn classify_input(input: &str) -> InputType {
    let trimmed = input.trim();
    
    // 1. Check against known commands whitelist
    if is_known_command(trimmed) {
        return parse_as_command(trimmed);
    }
    
    // 2. Heuristics for natural language detection
    if is_likely_natural_language(trimmed) {
        return InputType::NaturalLanguage(trimmed.to_string());
    }
    
    // 3. Check if looks like command syntax
    if looks_like_command(trimmed) {
        return parse_as_command(trimmed);
    }
    
    // 4. Default: treat as natural language
    InputType::NaturalLanguage(trimmed.to_string())
}
```

#### Heuristics for Classification

**Command indicators**:
- Single word or path (e.g., `ls`, `/usr/bin/ls`)
- Contains flags: `-`, `--` (e.g., `ls -la`)
- Starts with common commands: `cd`, `kubectl`, `aws`, `terraform`
- Contains pipes/redirects: `|`, `>`, `<`
- Environment variable syntax: `$VAR`, `${VAR}`

**Natural language indicators**:
- Multiple spaces between words
- Question words: "how", "what", "why", "when", "can you"
- Punctuation: `?`, `.`, `,`
- Articles: "a", "an", "the"
- Verbs without command structure: "show me", "explain", "help with"
- Length > 5 words without command syntax

**Whitelist approach** (recommended for M1):
Maintain a list of known DevOps commands:
```rust
const KNOWN_COMMANDS: &[&str] = &[
    // Basic shell
    "ls", "cd", "pwd", "cat", "echo", "grep", "find",
    // DevOps tools
    "kubectl", "helm", "terraform", "aws", "az", "gcloud",
    "docker", "docker-compose", "git",
    // Monitoring
    "top", "htop", "ps", "netstat", "curl", "wget",
];
```

#### Command Parsing

```rust
use shell_words;  // Crate for proper shell-style parsing

fn parse_as_command(input: &str) -> Option<InputType> {
    let parts = shell_words::split(input).ok()?;
    
    if parts.is_empty() {
        return None;
    }
    
    Some(InputType::Command(
        parts[0].clone(),
        parts[1..].to_vec()
    ))
}
```

**Recommended crate**: `shell-words` for proper handling of quotes, escapes, etc.

---

### M1.3: Bash Integration & Command Execution

#### Basic Command Execution

```rust
use std::process::{Command, Stdio};
use tokio::process::Command as TokioCommand;

async fn execute_command(
    cmd: &str, 
    args: &[String]
) -> Result<CommandOutput> {
    // 1. Check if command exists
    if !command_exists(cmd) {
        return Err(CommandError::NotFound(cmd.to_string()));
    }
    
    // 2. Execute asynchronously
    let output = TokioCommand::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    
    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

fn command_exists(cmd: &str) -> bool {
    which::which(cmd).is_ok()  // Use 'which' crate
}
```

**Recommended crates**:
- `which` - for finding executables in PATH
- `tokio` - for async command execution

#### Auto-Install Missing Commands

```rust
async fn handle_command_not_found(cmd: &str) -> Result<()> {
    // 1. Prompt user
    let prompt = format!(
        "Command '{}' not found. Install it? [y/N]: ", 
        cmd
    );
    
    let response = prompt_user(&prompt).await?;
    
    if !response.eq_ignore_ascii_case("y") {
        return Ok(());
    }
    
    // 2. Detect package manager and install
    install_package(cmd).await?;
    
    Ok(())
}

async fn install_package(package: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        // Try apt-get first (Debian/Ubuntu)
        if which::which("apt-get").is_ok() {
            return run_sudo_command(&[
                "apt-get", "install", "-y", package
            ]).await;
        }
        
        // Try yum (RedHat/CentOS)
        if which::which("yum").is_ok() {
            return run_sudo_command(&[
                "yum", "install", "-y", package
            ]).await;
        }
        
        // Try pacman (Arch)
        if which::which("pacman").is_ok() {
            return run_sudo_command(&[
                "pacman", "-S", "--noconfirm", package
            ]).await;
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        if which::which("brew").is_ok() {
            return run_command(&["brew", "install", package]).await;
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        if which::which("choco").is_ok() {
            return run_command(&["choco", "install", "-y", package]).await;
        }
        
        if which::which("winget").is_ok() {
            return run_command(&["winget", "install", package]).await;
        }
    }
    
    Err(Error::NoPackageManager)
}
```

**Important**: Handle sudo/privilege escalation carefully. May need to:
- Detect if sudo is required
- Prompt for password
- Use `sudo -S` for password via stdin

#### Basic Tab Completion (Simplified for M1)

**Don't implement full bash/zsh completion in M1** - too complex. Instead:

```rust
fn get_simple_completions(partial: &str) -> Vec<String> {
    let mut completions = Vec::new();
    
    // If no space, complete commands
    if !partial.contains(' ') {
        // Add PATH executables
        completions.extend(
            get_path_executables()
                .into_iter()
                .filter(|cmd| cmd.starts_with(partial))
        );
        
        // Add known commands
        completions.extend(
            KNOWN_COMMANDS
                .iter()
                .filter(|cmd| cmd.starts_with(partial))
                .map(|s| s.to_string())
        );
    } else {
        // Complete file paths
        completions.extend(complete_file_path(partial));
    }
    
    completions.sort();
    completions.dedup();
    completions
}

fn complete_file_path(partial: &str) -> Vec<String> {
    let parts: Vec<&str> = partial.rsplitn(2, ' ').collect();
    let path_part = parts[0];
    
    let (dir, prefix) = if path_part.contains('/') {
        let idx = path_part.rfind('/').unwrap();
        (&path_part[..=idx], &path_part[idx+1..])
    } else {
        (".", path_part)
    };
    
    let mut results = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(prefix) {
                    results.push(name.to_string());
                }
            }
        }
    }
    
    results
}
```

---

### M1.4: LLM Response Rendering

#### LLM Client Integration

```rust
use reqwest;  // For HTTP requests
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct LLMRequest {
    query: String,
    context: Option<String>,  // Optional: previous commands context
}

#[derive(Deserialize)]
struct LLMResponse {
    text: String,
    metadata: Option<serde_json::Value>,
}

struct LLMClient {
    base_url: String,
    client: reqwest::Client,
}

impl LLMClient {
    async fn query(&self, text: &str) -> Result<String> {
        let request = LLMRequest {
            query: text.to_string(),
            context: None,  // TODO: Add command history context
        };
        
        let response = self.client
            .post(&format!("{}/query", self.base_url))
            .json(&request)
            .send()
            .await?;
        
        let llm_response: LLMResponse = response.json().await?;
        
        Ok(llm_response.text)
    }
}
```

**Questions to clarify**:
- Backend API endpoint?
- Authentication mechanism?
- Request/response format?
- Timeout settings?
- Rate limiting?

#### Basic Markdown Rendering (M1 Scope)

**Don't implement full markdown parser in M1**. Focus on:
- Basic text formatting (bold, italic)
- Code blocks with minimal syntax highlighting
- Preserve newlines and structure

```rust
use syntect::easy::HighlightLines;
use syntect::parsing::SyntaxSet;
use syntect::highlighting::{ThemeSet, Style};

fn render_llm_response(text: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_lines = Vec::new();
    
    for line in text.lines() {
        // Detect code block start/end
        if line.starts_with("```") {
            if in_code_block {
                // End of code block - apply syntax highlighting
                output.extend(highlight_code(&code_lines, &code_lang));
                code_lines.clear();
                in_code_block = false;
            } else {
                // Start of code block
                code_lang = line.trim_start_matches("```").to_string();
                in_code_block = true;
            }
            continue;
        }
        
        if in_code_block {
            code_lines.push(line.to_string());
        } else {
            // Basic inline formatting
            let formatted = format_inline(line);
            output.push(formatted);
        }
    }
    
    output
}

fn format_inline(line: &str) -> String {
    let mut result = line.to_string();
    
    // Convert **bold** to ANSI bold
    // Simple regex replacement (use 'regex' crate)
    result = result.replace("**", "\x1b[1m");  // Bold on
    // TODO: Handle closing properly
    
    // Convert `code` to different color
    // result = result.replace("`", "\x1b[36m");  // Cyan
    
    result
}

fn highlight_code(lines: &[String], lang: &str) -> Vec<String> {
    // Use syntect for basic syntax highlighting
    // Limit to common languages for M1: rust, python, bash, json
    
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    
    let syntax = ps.find_syntax_by_extension(lang)
        .unwrap_or_else(|| ps.find_syntax_plain_text());
    
    let mut h = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);
    
    let mut output = Vec::new();
    for line in lines {
        let ranges = h.highlight_line(line, &ps).unwrap();
        let escaped = syntect::util::as_24_bit_terminal_escaped(&ranges[..], false);
        output.push(escaped);
    }
    
    output
}
```

**Recommended crates**:
- `syntect` - syntax highlighting (lightweight for M1)
- `regex` - for inline markdown patterns
- Avoid full markdown parsers (`pulldown-cmark`) in M1 - add in M2/M3

**M1 Rendering Limits**:
✅ Support:
- Plain text with newlines
- Code blocks with syntax highlighting (rust, python, bash, json only)
- Basic bold (`**text**`)
- Monospace inline code (`` `code` ``)

❌ Defer to later:
- Tables
- Lists (render as plain text)
- Images
- Links (just show URL as plain text)
- Nested formatting

---

## Project Structure

```
infraware-terminal/
├── Cargo.toml
├── src/
│   ├── main.rs              # Entry point + event loop
│   ├── terminal/
│   │   ├── mod.rs
│   │   ├── tui.rs          # ratatui rendering logic
│   │   ├── state.rs        # Terminal state management
│   │   └── events.rs       # Keyboard event handling
│   ├── input/
│   │   ├── mod.rs
│   │   ├── classifier.rs   # Command vs natural language
│   │   └── parser.rs       # Shell command parsing
│   ├── executor/
│   │   ├── mod.rs
│   │   ├── command.rs      # Command execution
│   │   ├── install.rs      # Auto-install logic
│   │   └── completion.rs   # Tab completion
│   ├── llm/
│   │   ├── mod.rs
│   │   ├── client.rs       # LLM API client
│   │   └── renderer.rs     # Response formatting
│   └── utils/
│       ├── mod.rs
│       ├── ansi.rs         # ANSI color utilities
│       └── errors.rs       # Error types
└── tests/
    ├── classifier_tests.rs
    ├── executor_tests.rs
    └── integration_tests.rs
```

---

## Dependencies (Cargo.toml)

```toml
[package]
name = "infraware-terminal"
version = "0.1.0"
edition = "2021"

[dependencies]
# TUI
ratatui = "0.29"
crossterm = "0.28"

# Async runtime
tokio = { version = "1.40", features = ["full"] }

# Command execution
which = "6.0"
shell-words = "1.1"

# LLM client
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Syntax highlighting
syntect = "5.2"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Utilities
regex = "1.10"
dirs = "5.0"  # For home directory, config paths

[dev-dependencies]
tokio-test = "0.4"
```

---

## Week-by-Week Timeline (M1)

### Week 1: TUI Foundation
**Goal**: Basic terminal that accepts input and displays output

**Tasks**:
- [x] Project setup (cargo init, dependencies)
- [ ] Basic TUI with ratatui
  - Input line at bottom
  - Scrollable output area above
  - Status bar showing mode
- [ ] Keyboard event capture (crossterm)
  - Character input
  - Enter to submit
  - Ctrl+C to interrupt
  - Arrow keys for history (basic)
- [ ] Output buffer management
  - Append new lines
  - Scroll handling
  - Color support (ANSI codes)

**Deliverable**: Terminal that echoes input back to output

---

### Week 2: Command Classification & Execution
**Goal**: Execute shell commands successfully

**Tasks**:
- [ ] Input classifier
  - Whitelist-based command detection
  - Heuristics for natural language
  - Tests for edge cases
- [ ] Command parser (shell-words)
- [ ] Basic command executor
  - Async execution with tokio
  - Capture stdout/stderr
  - Display exit codes
- [ ] Error handling
  - Command not found
  - Execution errors
  - Timeout handling

**Deliverable**: Can run commands like `ls -la`, `echo hello`, `pwd`

---

### Week 3: Auto-Install & LLM Integration
**Goal**: Handle missing commands and route natural language

**Tasks**:
- [ ] Command existence check (`which`)
- [ ] Auto-install flow
  - User prompt
  - Package manager detection
  - Installation execution
  - Post-install verification
- [ ] LLM client
  - HTTP client setup
  - Request/response handling
  - Error handling (timeout, network)
- [ ] Natural language routing
  - Detect phrases
  - Send to LLM
  - Display responses

**Deliverable**: 
- Auto-installs `htop` if missing
- Sends "how do I list files" to LLM and shows response

---

### Week 4: Polish & Testing
**Goal**: Stable, tested M1 MVP

**Tasks**:
- [ ] Basic markdown rendering
  - Code block detection
  - Syntax highlighting (4 languages)
  - Inline formatting
- [ ] Simple tab completion
  - Command completion
  - File path completion
- [ ] Cross-platform testing
  - Test on Linux
  - Test on macOS
  - Test on Windows (if in scope)
- [ ] Bug fixes and polish
  - Handle edge cases
  - Improve error messages
  - Performance optimization
- [ ] Basic documentation
  - README with usage
  - Architecture doc
  - Known issues list

**Deliverable**: Stable M1 that can demo to stakeholders

---

## Critical Questions to Answer ASAP

### Technical
1. **LLM Backend**:
   - Is the backend API ready?
   - What's the endpoint URL?
   - Request/response format?
   - Authentication method?
   - Rate limits?

2. **Environment**:
   - Where does this run? (Docker? Local? Cloud VM?)
   - What OS to prioritize? (Linux first, then macOS/Windows?)
   - Network restrictions?

3. **DevOps Tools**:
   - Priority list of commands to support?
   - Which cloud providers? (AWS, Azure, GCP?)
   - Specific tools required? (kubectl, terraform, helm, etc.)

### Product
4. **User Authentication**:
   - How do users authenticate?
   - How are cloud credentials (AWS/Azure) managed?
   - Stored locally? Via environment variables?

5. **Command Scope**:
   - Should ALL shell commands work?
   - Or only whitelisted DevOps commands?
   - How to handle dangerous commands? (rm -rf, etc.)

6. **LLM Interaction**:
   - Should LLM suggest commands?
   - Can LLM execute commands?
   - Or just provide information?

### Project Management
7. **Definition of Done for M1**:
   - What must work for stakeholder demo?
   - Acceptance criteria?
   - Performance requirements?

8. **Handoff**:
   - Who maintains after 3 months?
   - Documentation requirements?
   - Code review process?

9. **Infrastructure**:
   - CI/CD pipeline?
   - Testing environments?
   - Deployment strategy?

---

## Risk Assessment

### High Risk 🔴
1. **Input Classification Accuracy**: 
   - Distinguishing commands from natural language is ambiguous
   - Mitigation: Start with strict whitelist, expand gradually
   
2. **LLM Backend Dependency**:
   - If backend isn't ready, terminal is blocked
   - Mitigation: Create mock LLM for testing

3. **Cross-Platform Package Management**:
   - Different package managers per OS/distro
   - Mitigation: Focus on one OS for M1, add others in M2/M3

### Medium Risk 🟡
4. **Credential Management**:
   - Securely handling AWS/Azure credentials
   - Mitigation: Use standard credential chains, don't reinvent

5. **Command Execution Security**:
   - Arbitrary command execution is dangerous
   - Mitigation: Consider sandboxing, command approval

6. **Performance with Long Outputs**:
   - Large command outputs could freeze TUI
   - Mitigation: Stream output, add truncation/paging

### Low Risk 🟢
7. **Markdown Rendering**:
   - Basic rendering is sufficient for M1
   - Can enhance in later milestones

---

## Success Criteria for M1

✅ **Must Have**:
- [ ] TUI interface runs on Linux
- [ ] Executes basic shell commands (ls, cd, echo, etc.)
- [ ] Detects and auto-installs missing commands
- [ ] Routes natural language to LLM
- [ ] Displays LLM responses with basic formatting
- [ ] Handles errors gracefully (no crashes)
- [ ] Passes manual QA on target environment

✅ **Nice to Have**:
- [ ] Tab completion for commands
- [ ] Command history (up/down arrows)
- [ ] Syntax highlighting in code blocks
- [ ] Cross-platform support (macOS)

❌ **Out of Scope for M1**:
- Full markdown rendering (tables, images)
- Advanced completion (bash/zsh integration)
- Multi-shell support (Zsh - that's M3)
- Telemetry
- Advanced error recovery
- Performance optimization

---

## Next Steps

1. **Immediate**: Get answers to Critical Questions
2. **Setup**: Initialize project structure
3. **Week 1**: Start TUI foundation
4. **Weekly**: Review progress against timeline
5. **End of Week 3**: Internal demo
6. **End of Week 4**: Stakeholder demo

---

## Appendix: Useful Resources

### Rust TUI Development
- [ratatui documentation](https://ratatui.rs/)
- [ratatui examples](https://github.com/ratatui-org/ratatui/tree/main/examples)
- [crossterm guide](https://docs.rs/crossterm/latest/crossterm/)

### Command Execution
- [tokio::process](https://docs.rs/tokio/latest/tokio/process/index.html)
- [which crate](https://docs.rs/which/latest/which/)

### Syntax Highlighting
- [syntect examples](https://github.com/trishume/syntect/tree/master/examples)

### Similar Projects (for reference)
- [bottom](https://github.com/ClementTsang/bottom) - TUI process viewer
- [gitui](https://github.com/extrawurst/gitui) - Terminal UI for git
- [zellij](https://github.com/zellij-org/zellij) - Terminal workspace

---

## Contact & Clarifications

**For questions about**:
- Architecture decisions → [Technical Lead]
- LLM backend integration → [Backend Team]
- Product requirements → [Product Manager]
- Timeline concerns → [Project Manager]

**Weekly sync**: [To be scheduled]

---

*This document should be treated as a living specification. Update as requirements clarify and implementation progresses.*
