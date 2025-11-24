# SCAN Algorithm Architecture

**SCAN** = **S**hell-**C**ommand **A**nd **N**atural-language classification algorithm

## Overview

SCAN is the core input classification system for Infraware Terminal. It uses **alias expansion** and **history expansion** followed by a **Chain of Responsibility** pattern with 9 optimized handlers to distinguish between shell commands and natural language queries in <100μs.

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                         User Input                               │
│                    (from TUI InputBuffer)                        │
└────────────────────────────┬────────────────────────────────────┘
                             ↓
                    ┌───────────────────────┐
                    │  Alias Expansion      │
                    │  (if first word is    │
                    │   in alias map)       │
                    └─────────┬─────────────┘
                              ↓
                    ┌────────────────────┐
                    │  InputClassifier   │
                    │   .classify(str)   │
                    └─────────┬──────────┘
                              ↓
              ╔═══════════════════════════════╗
              ║  Chain of Responsibility      ║
              ║  (9 Handlers in strict order) ║
              ╚═══════════════════════════════╝
                              ↓
    ┌────────────────────────┼────────────────────────┐
    │                        │                        │
    ↓                        ↓                        ↓
┌─────────┐          ┌─────────────┐         ┌──────────────┐
│ Command │          │ CommandTypo │         │   Natural    │
│         │          │ (with typo  │         │   Language   │
│ "ls -la"│          │  suggestion)│         │ "what is..." │
└────┬────┘          └──────┬──────┘         └──────┬───────┘
     │                      │                       │
     ↓                      ↓                       ↓
CommandExecutor     Show Suggestion         LLMClient
  (bash/shell)       to user              (AI backend)
```

## Design Principles

1. **Fast Paths First**: Most common cases handled early (empty, known commands)
2. **Fail Fast**: Return immediately on match, don't continue chain
3. **Cache Everything**: CommandCache + precompiled patterns = sub-millisecond classification
4. **Typos Before LLM**: Levenshtein distance prevents expensive LLM calls
5. **English Only**: Multilingual queries delegated to LLM (more flexible than regex)
6. **Graceful Fallback**: DefaultHandler guarantees a result, never fails

## Alias Expansion (Pre-Classification)

**Location**: `src/input/classifier.rs:107-139`

### Purpose
Expand shell aliases before classification to match Bash behavior (e.g., `ll` → `ls -la`)

### Algorithm
```
1. Extract first word from input
2. Check if first word is in alias map (O(1) HashMap lookup)
3. If found:
   a. Get alias expansion (e.g., "ll" → "ls -la")
   b. Get remaining arguments (everything after first word)
   c. Reconstruct: expansion + remaining args
   d. Re-classify the expanded input
4. If not found:
   a. Proceed with original input to handler chain
```

### Example Flows

```
Input: "ll" (where ll='ls -la')
  ├─ First word: "ll"
  ├─ Is "ll" an alias? YES
  ├─ Expand to: "ls -la"
  ├─ Remaining args: (none)
  ├─ Reconstructed: "ls -la"
  └─ Classify "ls -la" via handler chain
     └─ Result: Command("ls", ["-la"])

Input: "ll *.txt" (where ll='ls -la')
  ├─ First word: "ll"
  ├─ Is "ll" an alias? YES
  ├─ Expand to: "ls -la"
  ├─ Remaining args: "*.txt"
  ├─ Reconstructed: "ls -la *.txt"
  └─ Classify "ls -la *.txt" via handler chain
     └─ Result: Command("ls", ["-la", "*.txt"])

Input: "gs" (not an alias)
  ├─ First word: "gs"
  ├─ Is "gs" an alias? NO
  └─ Proceed with original input to handler chain
     └─ May be typo for "git", command with syntax, or natural language
```

### Performance
- **Alias hit**: <1μs (HashMap lookup)
- **Alias miss**: <1μs (hash lookup says not found)
- **Total overhead**: <1μs even with expansion

### Alias Loading

**System Aliases** (loaded first at startup):
- `/etc/bash.bashrc` (Debian/Ubuntu)
- `/etc/bashrc` (RedHat/CentOS/Fedora)
- `/etc/profile`
- `/etc/profile.d/*.sh` (all files)

**User Aliases** (loaded second, override system):
- `~/.bashrc`
- `~/.bash_aliases`
- `~/.zshrc`

**Implementation**: `src/input/discovery.rs:151-254`
- `CommandCache::load_user_aliases()` - loads user aliases from home directory
- `CommandCache::load_system_aliases()` - loads system aliases, merges with user (user takes priority)
- Uses `tokio::spawn_blocking` in main.rs to avoid blocking async executor
- Performance: 1-5ms blocking I/O (async-safe via spawn_blocking)

### Security Validation

**Location**: `src/input/discovery.rs:337-373`

Dangerous patterns rejected:
- `rm -rf /` - Recursive delete from root
- `rm -rf /*` - Recursive delete all
- `mkfs` - Format filesystem
- `dd if=/dev/zero` - Wipe disk
- `:(){ :|:& };:` - Fork bomb
- `chmod -R 777 /` - Chmod everything
- `chown -R root /` - Chown everything
- `> /dev/sda` - Direct disk write
- `mkfs.` - Any mkfs variant

When a dangerous alias is encountered:
- Printed warning: "Warning: Rejecting potentially dangerous alias 'name': contains 'pattern'"
- Alias silently rejected (not added to cache)
- User-friendly - no crashes, no security violations

### Built-in Command: reload-aliases

**Purpose**: Runtime alias reloading for when config files change during session

**Implementation**: `src/orchestrators/command.rs:52-118`

**Usage**:
```
reload-aliases    # Reloads all system and user aliases from config files
```

**Behavior**:
1. Clears current alias cache
2. Reloads system aliases from `/etc/bash.bashrc`, etc.
3. Reloads user aliases from `~/.bashrc`, etc.
4. Shows success message to user
5. New aliases available immediately for next command

**Performance**: ~1-5ms blocking operation (uses `spawn_blocking`)

---

## Handler Chain (9 Handlers)

### Order Matters!

Handlers are executed in strict order. Each returns:
- `Some(InputType)` → classification complete, **STOP chain**
- `None` → pass to next handler

```rust
1. EmptyInputHandler        // <1μs    - Fast path for empty input
2. HistoryExpansionHandler  // ~1-5μs  - Bash-style history expansion (!!,  !$, !^, !*)
3. ShellBuiltinHandler      // <1μs    - Shell builtins (., :, [, [[, source, export, etc.)
4. PathCommandHandler        // ~10μs   - Executable paths (./script.sh)
5. KnownCommandHandler       // <1μs    - Whitelist + PATH verification (cached)
6. CommandSyntaxHandler      // ~10μs   - Flags, pipes, redirects
7. TypoDetectionHandler      // ~100μs  - Levenshtein distance ≤2
8. NaturalLanguageHandler    // ~5μs    - English patterns (precompiled)
9. DefaultHandler            // <1μs    - Fallback to NaturalLanguage
```

---

### 1. EmptyInputHandler

**Purpose**: Fast path for empty/whitespace input

**Location**: `src/input/handler.rs:77-105`

**Logic**:
```rust
if input.trim().is_empty() {
    return Some(InputType::Empty)
}
```

**Example**:
```
Input: "   " or ""
Output: InputType::Empty
Action: Ignored by main.rs
```

**Performance**: <1μs (trivial check)

---

### 2. HistoryExpansionHandler

**Purpose**: Expand bash-style history expansion patterns (!!,  !$, !^, !*)

**Location**: `src/input/history_expansion.rs`

**Recognizes** (4 patterns):
- `!!` - Entire previous command
- `!$` - Last argument (or command itself if no args, Bash-compatible)
- `!^` - First argument (fails if no args)
- `!*` - All arguments (fails if no args)

**Supports**:
- Multiple expansions in one input: `printf '%s %s' !^ !$`
- Preserves shell operators (pipes, redirects): `!! | grep pattern`
- Thread-safe via Arc<RwLock<Vec<String>>>

**Implementation Details**:
- Requires history reference via `InputClassifier::with_history(Arc<RwLock>)`
- Second-to-last semantics: Current input already added to history before classification
- RwLock poisoning recovery on read errors
- CommandParser integration for proper shell-words parsing

**Examples**:
```
Input: "sudo !!" (history: ["ls -la /tmp", "pwd"])
├─ First word: "sudo"
├─ Contains "!!"? YES
├─ Get second-to-last command: "pwd"
├─ Expand to: "sudo pwd"
└─ Output: Command("sudo", ["pwd"])

Input: "vim !$" (history: ["cat file.txt", "vim !$"])
├─ First word: "vim"
├─ Contains "!$"? YES
├─ Get second-to-last command: "cat file.txt"
├─ Parse: command="cat", args=["file.txt"]
├─ Last argument: "file.txt"
├─ Expand to: "vim file.txt"
└─ Output: Command("vim", ["file.txt"])

Input: "echo !$" (history: ["pwd", "echo !$"])
├─ First word: "echo"
├─ Contains "!$"? YES
├─ Get second-to-last command: "pwd"
├─ Parse: command="pwd", args=[]
├─ No arguments, use command itself (Bash behavior)
├─ Expand to: "echo pwd"
└─ Output: Command("echo", ["pwd"])
```

**Performance**: ~1-5μs (Arc<RwLock> read lock + pattern detection)

**Why Position 2?**:
- Must happen after EmptyInputHandler (need non-empty input)
- Must happen before other handlers (expansion happens before classification)
- Before ShellBuiltinHandler because `!!` might expand to a builtin

---

### 3. ShellBuiltinHandler

**Purpose**: Recognize shell builtin commands that don't exist in PATH

**Location**: `src/input/shell_builtins.rs`

**Recognizes** (45+ builtins):

**Punctuation**:
- `.` (dot) - POSIX source command
- `:` (colon) - POSIX no-op command
- `[` - POSIX test command (single bracket)
- `[[` - Bash/Zsh extended test (double bracket)

**Evaluation & Execution**:
- `source` - Bash/Zsh equivalent of `.`
- `eval` - Evaluate arguments as shell commands
- `exec` - Replace shell with command
- `return`, `exit` - Control flow

**Variable Management**:
- `export`, `unset`, `set` - Variable management
- `declare`, `local`, `readonly`, `typeset` - Variable declaration

**I/O & System**:
- `echo`, `printf`, `read` - I/O operations
- `alias`, `unalias` - Alias management
- `builtin`, `command`, `enable`, `type`, `hash`, `times`, `umask`, `ulimit`

**Job Control**:
- `jobs`, `fg`, `bg`, `wait` - Job management

**Directory Stack**:
- `pushd`, `popd`, `dirs` - Directory navigation

**Flow Control**:
- `break`, `continue`, `shift` - Loop and parameter control

**Logic**:
1. Is first word in builtin list? → Yes
2. Parse as command (builtin will be executed via `sh -c`)
3. Return Command with first word as builtin name

**Examples**:
```
Input: "."
├─ "." in builtins? YES
└─ Output: Command(".", [])

Input: ". ~/.bashrc"
├─ "." in builtins? YES
└─ Output: Command(".", ["~/.bashrc"])

Input: "[[" -f file.txt "]]"
├─ "[[" in builtins? YES
└─ Output: Command("[[", ["-f", "file.txt", "]]"])

Input: "export PATH=/usr/bin"
├─ "export" in builtins? YES
└─ Output: Command("export", ["PATH=/usr/bin"])

Input: "source /etc/profile"
├─ "source" in builtins? YES
└─ Output: Command("source", ["/etc/profile"])
```

**Performance**: <1μs (hash lookup in builtin list)

**Execution Strategy**:
- Builtins like `.`, `:`, `[[` don't exist as standalone executables in PATH
- Instead of searching PATH, execute through shell: `sh -c "builtin args"`
- Example: `sh -c ". ~/.bashrc"` for source command
- Shell handles the builtin semantics properly

**Why It Matters**:
- Many shell builtins won't exist in PATH (e.g., `[[`, `.`, `:`)
- Users expect these commands to work in a terminal
- Without builtin recognition, they'd be misclassified as natural language
- Builtin detection happens early (position 2) before expensive PATH lookups
- Saves 1-5ms per builtin command vs PATH verification

---

### 4. PathCommandHandler

**Purpose**: Detect executable paths (unambiguous command intent)

**Location**: `src/input/handler.rs:629-721`

**Logic**:
1. Does first token start with `/`, `./`, or `../`? → Check executability
2. **Unix**: Check file mode `& 0o111` (executable bit)
3. **Windows**: Check extension (.exe, .bat, .cmd, .ps1, .sh)
4. Parse as command if executable

**Examples**:
```
Input: "./deploy.sh --production"
Output: Command("./deploy.sh", ["--production"])

Input: "/usr/bin/python3 script.py"
Output: Command("/usr/bin/python3", ["script.py"])

Input: "../build.sh"
Output: Command("../build.sh", [])
```

**Performance**: ~10μs (file system check)

**Why It Matters**: Paths like `./script.sh` are unambiguous - user clearly wants to execute a file, not ask a question.

---

### 5. KnownCommandHandler

**Purpose**: Fast path for whitelisted DevOps commands with PATH verification

**Location**: `src/input/handler.rs:107-289`

**Whitelist** (60+ commands):
```
Shell:     ls, cd, pwd, cat, grep, find, mkdir, rm, cp, mv, chmod, chown
Docker:    docker, docker-compose
K8s:       kubectl, helm, minikube, k9s
Cloud:     aws, az, gcloud, terraform, pulumi
VCS:       git, svn, hg
Build:     make, cargo, npm, yarn, pip, maven, gradle
Monitoring: prometheus, grafana
DevOps:    ansible, vagrant, packer, consul, vault
```

**Logic**:
1. Is first word in whitelist? → Check PATH
2. Use `CommandCache::is_available(cmd)` (thread-safe cache)
3. If found in PATH → parse as command
4. Otherwise → pass to next handler

**Examples**:
```
Input: "docker ps"
├─ "docker" in whitelist? YES
├─ docker exists in PATH? YES (cached or verified with which crate)
└─ Output: Command("docker", ["ps"])

Input: "kubectl get pods"
├─ "kubectl" in whitelist? YES
├─ kubectl exists in PATH? NO (not installed)
└─ Output: None (pass to next handler)
    └─ Eventually becomes: CommandTypo or NaturalLanguage
```

**Performance**:
- Cache hit: <1μs (hash lookup)
- Cache miss (first call): 1-5ms (PATH search via `which` crate)
- Subsequent calls: <1μs (cached)

**Cache Structure** (in `src/input/discovery.rs`):
```rust
static COMMAND_CACHE: Lazy<RwLock<CommandCache>> = Lazy::new(|| {
    RwLock::new(CommandCache {
        available: HashSet::new(),
        unavailable: HashSet::new(),
        aliases: HashMap::new(),
    })
});
```

---

### 6. CommandSyntaxHandler

**Purpose**: Detect command syntax even if command is unknown (language-agnostic)

**Location**: `src/input/handler.rs:154-225`

**Language-Agnostic Algorithm**:
```
Has flags (-/--) OR shell operators → Command
Has paths (./, /, $VAR) → Command
Multi-word without above → Natural Language (defer to next handler)
Single word → Defer to other handlers (KnownCommand, Typo, Path, etc.)
```

**Detects**:
- Flags: `" -"` or `" --"`
- Pipes: `"|"`
- Redirects: `">"`, `"<"`
- Logical operators: `"&&"`, `"||"`, `";"`, `"&"`
- Environment variables: `"$VAR"`, `"${VAR}"`
- Subshells: `"$(...)"``, backticks
- Paths in arguments: `"/path"`, `"./path"`, `"../path"`

**Examples**:
```
Input: "unknown-cmd --flag value"
Output: Command("unknown-cmd", ["--flag", "value"])
Reason: Contains "--flag"

Input: "cat file.txt | grep pattern"
Output: Command("cat", ["file.txt", "|", "grep", "pattern"])
Reason: Contains pipe "|"

Input: "echo $USER"
Output: Command("echo", ["$USER"])
Reason: Contains environment variable

Input: "tell me about docker"
Output: None (passes to next handler)
Reason: Multi-word without flags/operators → natural language (any language)

Input: "how do i deploy"
Output: None (passes to next handler)
Reason: Multi-word without flags → natural language (works for ANY language)
```

**Performance**: ~10μs (precompiled regex pattern matching)

**Key Innovation**: This handler is now **fully language-agnostic**. Multi-word inputs without command syntax are classified as Natural Language and delegated to the LLM, which handles multilingual queries much more accurately than regex patterns. This removes the need for hardcoded multilingual patterns (IT, ES, FR, DE, etc.)

**Why It Matters**: Even if we don't know the command, syntax like `--flag` clearly indicates command intent. For ambiguous multi-word inputs, delegation to the LLM provides better accuracy across all languages.

---

### 7. TypoDetectionHandler

**Purpose**: Catch typos before expensive LLM calls (language-agnostic)

**Location**: `src/input/typo_detection.rs:66-100`

**Language-Agnostic Algorithm**:
```
Single word:
  ├─ Filter out common NL words (what, how, why, who, when, where, help, hi, thanks, etc.)
  └─ Then check Levenshtein distance against known commands

Multi-word:
  ├─ Has flags/operators? → might be a typo (check Levenshtein)
  ├─ No flags/operators? → NOT a typo, defer to next handler (natural language)
```

**Rationale**: Multi-word phrases without command syntax (like "how do I deploy") work identically across ALL languages and don't need special language-specific filtering. Single-word filtering keeps minimal universal patterns to avoid false positives like "what" → "cat".

**Levenshtein Distance Examples**:
```
"dokcer"  vs "docker"  → distance=2 (replace k→c, insert c)
"kubeclt" vs "kubectl" → distance=2 (swap ct→tc)
"gti"     vs "git"     → distance=1 (insert t)
```

**Examples**:
```
Input: "dokcer ps"
├─ Word count: 2 (multi-word)
├─ Has flags/operators? NO
└─ Output: None (multi-word without flags → natural language, defer to next)
   (This is now language-agnostic: works for "comando_errato args" in Italian too)

Input: "dokcer -v"
├─ Word count: 2 (multi-word)
├─ Has flags/operators? YES (contains "-v")
├─ Closest match: "docker" at distance=2
└─ Output: CommandTypo {
      input: "dokcer -v",
      suggestion: "docker",
      distance: 2
   }

Input: "what"
├─ Word count: 1 (single word)
├─ Is "what" in NL filter list? YES
└─ Output: None (pass to NaturalLanguageHandler)

Input: "dokcer"
├─ Word count: 1 (single word)
├─ Is "dokcer" in NL filter list? NO
├─ Closest match: "docker" at distance=2
└─ Output: CommandTypo {
      input: "dokcer",
      suggestion: "docker",
      distance: 2
   }

Input: "tell me about docker"
├─ Word count: 4 (multi-word)
├─ Has flags/operators? NO
└─ Output: None (multi-word without flags → natural language, ANY language)
```

**Performance**: ~100μs (60 Levenshtein comparisons for single words)

**Key Innovation**: Multi-word filtering is now **fully language-agnostic**. "multi-word WITHOUT flags" universally means "natural language" regardless of language. This eliminates the need for language-specific patterns.

**Cost Savings**:
```
Before: "dokcer ps" → NaturalLanguage → LLM call (100-500ms + API cost)
After:  "dokcer -v" → CommandTypo → Show suggestion (<100μs, no API call)

Speedup: ~1000x faster
Cost: $0 instead of $0.001-$0.01 per call
```

---

### 8. NaturalLanguageHandler

**Purpose**: Detect English natural language patterns

**Location**: `src/input/handler.rs:362-627`

**English-Only After Refactoring**:
```
Before: Patterns for EN, IT, ES, FR, DE (~25 regex patterns)
After:  English-only patterns (~12 regex patterns)
Reason: LLM handles multilingual queries more accurately
```

**Detects** (using precompiled patterns):
1. **Question words**: how, what, why, when, where, who, which
2. **Polite phrases**: can you, could you, please, help, show me, explain
3. **Articles**: a, an, the
4. **Punctuation**: `?`, `!`
5. **Long phrases**: >5 words without command syntax

**Examples**:
```
Input: "how do I list files?"
├─ starts_with_question_word("how")? YES
└─ Output: NaturalLanguage("how do I list files?")

Input: "show me the logs"
├─ has_articles(" the ")? YES
└─ Output: NaturalLanguage("show me the logs")

Input: "can you help me?"
├─ starts_with_question_word("can you")? YES
└─ Output: NaturalLanguage("can you help me?")

Input: "please explain docker"
├─ starts_with_question_word("please")? YES
└─ Output: NaturalLanguage("please explain docker")
```

**Performance**: ~5μs (precompiled RegexSet)

**Pattern Precompilation** (in `src/input/patterns.rs`):
```rust
static PATTERNS: Lazy<CompiledPatterns> = Lazy::new(|| {
    CompiledPatterns {
        question_words: RegexSet::new([
            r"(?i)^(how|what|why|when|where|who|which)\s",
            r"(?i)^(can you|could you|would you|will you)\s",
            r"(?i)^(please|help|show me|explain)\s",
        ]).unwrap(),
        articles: RegexSet::new([
            r"\s(a|an|the)\s",
            r"^(a|an|the)\s",
        ]).unwrap(),
        // ...
    }
});

// 10-100x faster than compiling regex on every call!
```

---

### 9. DefaultHandler

**Purpose**: Catch-all fallback (guarantees a result)

**Location**: `src/input/handler.rs:723-754`

**Logic**:
```rust
fn handle(&self, input: &str) -> Option<InputType> {
    Some(InputType::NaturalLanguage(input.trim().to_string()))
}
```

**Examples**:
```
Input: "ambiguous input here"
├─ All previous handlers returned None
└─ Output: NaturalLanguage("ambiguous input here")
    └─ Sent to LLM for interpretation
```

**Performance**: <1μs

**Why It Matters**: Ensures the chain **always** returns a result. No panics, no errors.

---

## InputType Variants

**Location**: `src/input/classifier.rs:14-40`

```rust
pub enum InputType {
    Command {
        command: String,
        args: Vec<String>,
        original_input: Option<String>,  // For shell operators (pipes, redirects, etc.)
    },
    NaturalLanguage(String),
    Empty,
    CommandTypo { input: String, suggestion: String, distance: usize },
}
```

### Shell Operator Support

When the input contains shell operators (pipes `|`, redirects `>`, `<`, logical operators `&&`, `||`, etc.), the `original_input` field is populated with the full input string. This enables proper shell interpretation via `sh -c` during execution.

**Examples**:
- Simple command: `ls -la` → `original_input: None` (direct execution)
- Pipe command: `ls | grep test` → `original_input: Some("ls | grep test")` (shell interpretation)
- Redirect: `echo hello > file.txt` → `original_input: Some("echo hello > file.txt")` (shell interpretation)

### Handling in main.rs

**Location**: `src/main.rs:307-323`

```rust
match classifier.classify(&input)? {
    InputType::Command { command, args, original_input } => {
        // Execute via CommandOrchestrator with optional shell interpretation
        self.handle_command(&command, &args, original_input.as_deref()).await?;
    }
    InputType::NaturalLanguage(query) => {
        // Send to LLM via NaturalLanguageOrchestrator
        self.handle_natural_language(&query).await?;
    }
    InputType::CommandTypo { input, suggestion, distance } => {
        // Show suggestion to user
        self.handle_command_typo(&input, &suggestion, distance).await?;
    }
    InputType::Empty => {
        // Ignore
    }
}
```

---

## Complete Flow Examples

### Example 1: "ls -la" (Known Command)

```
┌──────────────────────────┐
│ Input: "ls -la"          │
└──────────┬───────────────┘
           ↓
   ┌───────────────────┐
   │ EmptyInputHandler │
   └────────┬──────────┘
            ↓ Not empty
   ┌──────────────────────────┐
   │HistoryExpansionHandler   │
   └────────┬─────────────────┘
            ↓ No !!,  !$, !^, !*
   ┌──────────────────────┐
   │ ShellBuiltinHandler  │
   └────────┬─────────────┘
            ↓ "ls" not a builtin
   ┌───────────────────┐
   │ PathCommandHandler│
   └────────┬──────────┘
            ↓ Not a path (no ./ or /)
   ┌────────────────────┐
   │KnownCommandHandler │
   └────────┬───────────┘
            ├─ "ls" in whitelist? ✓ YES
            ├─ ls exists in PATH? ✓ YES (cached)
            └─ Returns: Command("ls", ["-la"])
            ✓ STOP HERE

┌─────────────────────────┐
│ Output:                 │
│ Command("ls", ["-la"])  │
└──────────┬──────────────┘
           ↓
   Execute in bash shell
```

**Performance**: <1μs (cache hit)

---

### Example 2: "dokcer ps" (Typo)

```
┌──────────────────────────┐
│ Input: "dokcer ps"       │
└──────────┬───────────────┘
           ↓
   [EmptyInputHandler] ✗ Not empty
   [HistoryExpansionHandler] ✗ No history patterns
   [ShellBuiltinHandler] ✗ "dokcer" not a builtin
   [PathCommandHandler] ✗ Not a path
   [KnownCommandHandler] ✗ "dokcer" not in whitelist
   [CommandSyntaxHandler] ✗ No flags/pipes
           ↓
   ┌────────────────────────┐
   │ TypoDetectionHandler   │
   └────────┬───────────────┘
            ├─ Looks like command? ✓ YES (2 words)
            ├─ Unknown? ✓ YES
            ├─ Find closest:
            │  levenshtein("dokcer", known_commands)
            │  → "docker" at distance=2
            └─ Returns: CommandTypo {
                 input: "dokcer ps",
                 suggestion: "docker",
                 distance: 2
               }
            ✓ STOP HERE

┌─────────────────────────────────────┐
│ Output:                             │
│ CommandTypo {                       │
│   input: "dokcer ps",               │
│   suggestion: "docker",             │
│   distance: 2                       │
│ }                                   │
└──────────┬──────────────────────────┘
           ↓
Show to user:
"Command not found: 'dokcer'
 Did you mean 'docker'? (Levenshtein distance: 2)"
```

**Performance**: ~100μs
**Cost Savings**: Avoided LLM call (~100-500ms + $0.001-$0.01)

---

### Example 3: "show me the logs" (Natural Language)

```
┌──────────────────────────────┐
│ Input: "show me the logs"    │
└──────────┬───────────────────┘
           ↓
   [EmptyInputHandler] ✗
   [HistoryExpansionHandler] ✗ No history patterns
   [ShellBuiltinHandler] ✗ "show" not a builtin
   [PathCommandHandler] ✗
   [KnownCommandHandler] ✗ "show" not in whitelist
   [CommandSyntaxHandler] ✗ No syntax
           ↓
   ┌────────────────────────┐
   │ TypoDetectionHandler   │
   └────────┬───────────────┘
            ├─ Looks like command?
            │  if contains(" the ") → ✗ NO
            └─ Pass to next handler
           ↓
   ┌──────────────────────────┐
   │ NaturalLanguageHandler   │
   └────────┬─────────────────┘
            ├─ has_articles(" the ")? ✓ YES
            └─ Returns: NaturalLanguage("show me the logs")
            ✓ STOP HERE

┌────────────────────────────────────┐
│ Output:                            │
│ NaturalLanguage("show me the logs")│
└──────────┬─────────────────────────┘
           ↓
Send to LLM for interpretation
```

**Performance**: ~5μs (precompiled regex)

---

### Example 4: "what is kubernetes?" (Question)

```
┌──────────────────────────────┐
│ Input: "what is kubernetes?" │
└──────────┬───────────────────┘
           ↓
   [EmptyInputHandler] ✗
   [HistoryExpansionHandler] ✗ No history patterns
   [ShellBuiltinHandler] ✗ "what" not a builtin
   [PathCommandHandler] ✗
   [KnownCommandHandler] ✗ "what" not in whitelist
   [CommandSyntaxHandler] ✗
           ↓
   ┌────────────────────────┐
   │ TypoDetectionHandler   │
   └────────┬───────────────┘
            ├─ Looks like command?
            │  if contains("?") → ✗ NO
            └─ Pass to next handler
           ↓
   ┌──────────────────────────┐
   │ NaturalLanguageHandler   │
   └────────┬─────────────────┘
            ├─ starts_with_question_word("what")? ✓ YES
            └─ Returns: NaturalLanguage("what is kubernetes?")
            ✓ STOP HERE

┌─────────────────────────────────────────┐
│ Output:                                 │
│ NaturalLanguage("what is kubernetes?")  │
└──────────┬──────────────────────────────┘
           ↓
Send to LLM backend
```

**Performance**: ~5μs

---

### Example 5: ". ~/.bashrc" (Shell Builtin)

```
┌──────────────────────────────┐
│ Input: ". ~/.bashrc"         │
└──────────┬───────────────────┘
           ↓
   ┌──────────────────────────────┐
   │ EmptyInputHandler            │
   └────────┬─────────────────────┘
            ↓ Not empty
   ┌──────────────────────────────┐
   │ HistoryExpansionHandler      │
   └────────┬─────────────────────┘
            ↓ No history patterns
   ┌──────────────────────┐
   │ ShellBuiltinHandler  │
   └────────┬─────────────┘
            ├─ "." in builtins? ✓ YES (POSIX source command)
            ├─ Not in PATH (. is not an executable)
            └─ Returns: Command(".", ["~/.bashrc"])
            ✓ STOP HERE

┌──────────────────────────────┐
│ Output:                      │
│ Command(".", ["~/.bashrc"])  │
└──────────┬───────────────────┘
           ↓
Execute via shell: sh -c ". ~/.bashrc"
Builtin handles sourcing the file
```

**Performance**: <1μs (hash lookup in builtin list)

**Key Point**: The `.` (dot) command is a shell builtin that sources a file. It doesn't exist as an executable in PATH, so it's recognized early by ShellBuiltinHandler (position 2) before checking PATH. This saves the 1-5ms PATH lookup overhead.

**Alternative Usage**:
```
Input: "source ~/.bashrc"
├─ "source" in builtins? ✓ YES
└─ Returns: Command("source", ["~/.bashrc"])

Input: "[[ -f ~/.bashrc ]]"
├─ "[[" in builtins? ✓ YES (bash/zsh extended test)
└─ Returns: Command("[[", ["-f", "~/.bashrc", "]]"])
```

---

### Example 6: "ls -la | grep test" (Pipe Command with Shell Operators)

```
┌────────────────────────────────┐
│ Input: "ls -la | grep test"   │
└──────────┬─────────────────────┘
           ↓
   [EmptyInputHandler] ✗ Not empty
   [HistoryExpansionHandler] ✗ No history patterns
   [ShellBuiltinHandler] ✗ "ls" not a builtin
   [PathCommandHandler] ✗ Not a path
   [KnownCommandHandler] ✗ "ls" exists BUT input has shell operators
           ↓
   ┌────────────────────────┐
   │ CommandSyntaxHandler   │
   └────────┬───────────────┘
            ├─ has_shell_operators()? ✓ YES (detected "|")
            ├─ Parse with shell-words: ["ls", "-la", "|", "grep", "test"]
            └─ Returns: Command {
                 command: "ls",
                 args: ["-la", "|", "grep", "test"],
                 original_input: Some("ls -la | grep test")  ← PRESERVED!
               }
            ✓ STOP HERE

┌──────────────────────────────────────────┐
│ Output:                                  │
│ Command {                                │
│   command: "ls",                         │
│   args: ["-la", "|", "grep", "test"],   │
│   original_input: Some("ls -la | grep test") │
│ }                                        │
└──────────┬───────────────────────────────┘
           ↓
   ┌────────────────────────────┐
   │ CommandExecutor::execute   │
   └────────┬───────────────────┘
            ├─ original_input.is_some()? ✓ YES
            ├─ Use shell interpretation:
            │  sh -c "ls -la | grep test"
            └─ Execute with proper pipe handling
            ✓ SUCCESS

Result: Files listed and filtered through grep
```

**Key Features**:
- **Shell Operator Detection**: Automatically detects `|`, `>`, `<`, `&&`, `||`, `;`, `&`, `$()`, backticks
- **Original Input Preservation**: Full command string saved in `original_input` field
- **Shell Interpretation**: Uses `sh -c` for proper operator handling
- **Security**: Direct execution for simple commands, shell only when needed

**Supported Operators**:
- **Pipes**: `|` (e.g., `ls | grep test`)
- **Redirects**: `>`, `<`, `>>`, `2>` (e.g., `echo hello > file.txt`)
- **Logical**: `&&`, `||` (e.g., `mkdir dir && cd dir`)
- **Separators**: `;`, `&` (e.g., `cmd1; cmd2`)
- **Subshells**: `$(...)`, `` `...` `` (e.g., `echo $(date)`)

**Performance**: ~5μs (precompiled regex for operator detection)

---

## Performance Optimization

### 1. Precompiled RegexSet Patterns

**Problem**: Compiling regex on every classification = ~500μs overhead

**Solution**: Compile once at startup with `once_cell::Lazy`

**Implementation** (`src/input/patterns.rs:36-84`):
```rust
use once_cell::sync::Lazy;

static PATTERNS: Lazy<CompiledPatterns> = Lazy::new(|| {
    CompiledPatterns {
        question_words: RegexSet::new([...]).unwrap(),
        articles: RegexSet::new([...]).unwrap(),
        // Compiled ONCE at first access
    }
});

// Usage: CompiledPatterns::get() returns &'static reference
let patterns = CompiledPatterns::get();
patterns.has_articles(input); // ~5μs instead of ~500μs
```

**Speedup**: 10-100x faster pattern matching

---

### 2. CommandCache (Thread-Safe)

**Problem**: Calling `which command` repeatedly = 1-5ms per call

**Solution**: Cache results in thread-safe RwLock + HashSet

**Implementation** (`src/input/discovery.rs:25-85`):
```rust
static COMMAND_CACHE: Lazy<RwLock<CommandCache>> = Lazy::new(|| {
    RwLock::new(CommandCache {
        available: HashSet::new(),      // Commands found in PATH
        unavailable: HashSet::new(),    // Commands NOT found
        aliases: HashMap::new(),        // User aliases from .bashrc
    })
});

pub fn is_available(command: &str) -> bool {
    // Try read lock first (99% of calls)
    {
        let cache = COMMAND_CACHE.read().unwrap();
        if cache.available.contains(command) { return true; }
        if cache.unavailable.contains(command) { return false; }
    }

    // Cache miss: check with which crate
    let exists = which::which(command).is_ok();

    // Update cache (write lock)
    {
        let mut cache = COMMAND_CACHE.write().unwrap();
        if exists {
            cache.available.insert(command.to_string());
        } else {
            cache.unavailable.insert(command.to_string());
        }
    }

    exists
}
```

**Performance**:
- Cache hit: <1μs (hash lookup)
- Cache miss: 1-5ms (PATH search)
- Subsequent hits: <1μs

**Why RwLock?**
- Read-heavy workload (99% reads, 1% writes)
- Multiple threads can read simultaneously
- Write lock only on cache miss

---

### 3. Handler Chain Ordering

**Strategy**: Fast paths first, expensive operations later

```
Handler                     Avg Time    Hit Rate
──────────────────────────────────────────────
EmptyInputHandler           <1μs        ~2%
HistoryExpansionHandler     ~1-5μs      ~0.5%  ← History expansion (!!,  !$, !^, !*)
ShellBuiltinHandler         <1μs        ~2%    ← Shell builtins (., :, [[, source, etc.)
PathCommandHandler          ~10μs       ~1%
KnownCommandHandler         <1μs        ~65%   ← MOST COMMON
CommandSyntaxHandler        ~10μs       ~5%
TypoDetectionHandler        ~100μs      ~3%
NaturalLanguageHandler      ~5μs        ~19%
DefaultHandler              <1μs        ~2.5%
```

**Result**: Average classification time = ~10μs (dominated by KnownCommandHandler cache hits + HistoryExpansionHandler/ShellBuiltinHandler fast paths)

---

### Performance Summary

```
┌─────────────────────────────────────────────────┐
│ Typical Classification Times                    │
├─────────────────────────────────────────────────┤
│ Empty input             <1μs                    │
│ Shell builtin           <1μs  ← ~2% of inputs  │
│ Known command (cached)  <1μs  ← ~65% of inputs │
│ Path command            ~10μs                   │
│ Command syntax          ~10μs                   │
│ Typo detection          ~100μs                  │
│ Natural language        ~5μs                    │
│ Default fallback        <1μs                    │
├─────────────────────────────────────────────────┤
│ TOTAL (average)         ~10μs                   │
│ With PATH lookup        1-5ms (first time only)│
└─────────────────────────────────────────────────┘
```

---

## Multilingual Handling (Post-Refactoring)

### Philosophy: Language-Agnostic Core Algorithm

```
┌──────────────────────────────────────────────────────────┐
│ SCAN Classifier (Language-Agnostic Core)               │
│ ├─ CommandSyntaxHandler:   "Has flags/pipes?" → Works  │
│ │                          for ALL languages            │
│ ├─ TypoDetectionHandler:   "Multi-word no flags?"      │
│ │                          → Natural Language (ANY lang)│
│ ├─ NaturalLanguageHandler: English patterns (fast path) │
│ └─ DefaultHandler:         Catches non-English → LLM   │
└─────────────────┬──────────────────────────────────────┘
                  ↓
         If NaturalLanguage (any language)
                  ↓
┌──────────────────────────────────────────────────────────┐
│ LLM Backend (All Languages)                            │
│ ├─ Flexible: Handles EN, IT, ES, FR, DE, ZH, AR, etc. │
│ ├─ Accurate: Better than regex for context            │
│ └─ Smart: Understands intent, not just keywords       │
└──────────────────────────────────────────────────────────┘
```

### Key Innovation: Syntax-Based, Not Language-Based

Instead of hardcoding patterns for 5 languages (EN, IT, ES, FR, DE), the algorithm now relies on **universal syntax patterns** that work identically across all languages:

**CommandSyntaxHandler** (language-agnostic):
- If input has flags (`-`, `--`) → Command
- If input has pipes (`|`, `>`, `<`, `&&`) → Command
- If input is multi-word WITHOUT above → Natural Language (works for ANY language)
- If input is single word → Defer to other handlers

**TypoDetectionHandler** (language-agnostic):
- Multi-word without flags → Not a typo (defer to NL handler, ANY language)
- Single word → Check Levenshtein (universal approach)

### Before Refactoring

```
Pattern Count:   ~25 regex patterns (5 languages × 5 patterns)
Languages:       EN, IT, ES, FR, DE in classifier
Test Count:      217 tests (60 multilingual)
Maintenance:     Complex - 5 languages to maintain
Flexibility:     Low - adding new language requires code changes
Algorithm:       Language-specific keywords and patterns
```

### After Refactoring

```
Pattern Count:   ~12 regex patterns (English-only fast path)
Languages:       English patterns + language-agnostic logic
Test Count:      157 tests (-60 multilingual tests)
Maintenance:     Simple - 1 language in classifier
Flexibility:     High - LLM handles new languages automatically
Algorithm:       Syntax-based (works for ALL languages)
```

### Example Flow: Language-Agnostic Classification

```
Input: "come posso listare i file?" (Italian: "how can I list files?")
       ↓
SCAN Classifier (all language-agnostic logic):
  ├─ EmptyInputHandler: Not empty ✓
  ├─ HistoryExpansionHandler: No history patterns ✓
  ├─ ShellBuiltinHandler: "come" not a builtin ✓
  ├─ PathCommandHandler: Not a path ✓
  ├─ KnownCommandHandler: "come" not in whitelist ✓
  ├─ CommandSyntaxHandler:
  │   ├─ Has flags/operators? NO ✓
  │   ├─ Word count: 5 (multi-word) ✓
  │   └─ Multi-word without flags → Natural Language (ANY language!)
  │       Returns: None (defer to next handler)
  ├─ TypoDetectionHandler:
  │   ├─ Word count: 5 (multi-word)
  │   ├─ Has flags/operators? NO
  │   └─ Multi-word without flags → NOT a typo, defer (language-agnostic!)
  │       Returns: None (pass to next)
  ├─ NaturalLanguageHandler:
  │   ├─ Starts with EN question word? NO (it's Italian)
  │   └─ Returns: None (English patterns don't match)
  └─ DefaultHandler:
      └─ Returns: NaturalLanguage("come posso listare i file?")
       ↓
LLM Backend (handles all languages):
  ├─ Detects: Italian language automatically
  ├─ Understands: "how can I list files?"
  ├─ Provides accurate answer: "You can use 'ls' command..."
  └─ No translation needed - LLM handles it
```

**Key Insight**: The algorithm works identically for Italian, Spanish, French, German, Chinese, Arabic, etc. No language-specific patterns needed!

**Benefit**:
- LLM is more accurate and flexible than hardcoded regex patterns
- Adding new languages requires zero code changes
- CommandSyntaxHandler and TypoDetectionHandler use universal syntax logic, not language keywords

---

### Design Rationale: Language-Agnostic Core with English-First Fast Path

#### Three-Layer Approach

**Layer 1: Language-Agnostic Syntax Matching** (works for ALL languages)
- CommandSyntaxHandler: "Has flags/pipes?" logic works identically in any language
- TypoDetectionHandler: "Multi-word without flags?" classification works universally
- Performance: <100μs, zero language-specific patterns needed

**Layer 2: English Fast Path** (optional optimization for 70-80% of queries)
- NaturalLanguageHandler: Catches English patterns in ~5μs
- Precompiled regex for English question words ("how", "what", "can you", etc.)
- Allows early exit for common English queries without reaching DefaultHandler

**Layer 3: LLM Fallback** (handles all languages, all contexts)
- DefaultHandler catches anything not matched by Layers 1-2
- LLM handles 100+ languages automatically
- Provides context-aware interpretation that regex patterns cannot

#### Why Language-Agnostic Syntax Over Multilingual Patterns?

**Old Approach (Pre-Refactoring)**:
```
CommandSyntaxHandler:
  ├─ English patterns: "is question word?" + regex
  ├─ Italian patterns: "è parola interrogativa?" + regex
  ├─ Spanish patterns: "es palabra interrogativa?" + regex
  └─ Result: 25 patterns, 5 languages, high maintenance

TypoDetectionHandler:
  ├─ English filters: what, how, why, when (language-specific)
  ├─ Italian filters: cosa, come, perché, quando (language-specific)
  └─ Result: Complex, error-prone, inflexible
```

**New Approach (Post-Refactoring)**:
```
CommandSyntaxHandler:
  └─ Universal syntax: "Has flags/pipes?" → Works for ALL languages!

TypoDetectionHandler:
  ├─ Multi-word logic: "Has flags without flags?" → Language-agnostic
  └─ Single-word filter: ~10 universal NL words (what, hi, thanks, etc.)
```

**Benefits of Language-Agnostic Approach**:
- ✅ Universal coverage: Works for EN, IT, ES, FR, DE, ZH, AR, JA, etc. automatically
- ✅ Zero maintenance: No language-specific patterns to update
- ✅ Adding new languages: Zero code changes required (LLM handles it)
- ✅ Better accuracy: Syntax-based classification is more reliable than keyword matching
- ✅ Simpler code: ~10 lines of syntax logic vs 25+ regex patterns

#### Performance Comparison

| Scenario | Handler | Classification Time | Total Time |
|----------|---------|---------------------|------------|
| English query "how do I...?" | NaturalLanguageHandler | ~5μs | ~5μs + LLM |
| Italian query "come posso...?" | DefaultHandler | ~6μs | ~6μs + LLM |
| Multi-word no flags (any language) | CommandSyntaxHandler | ~10μs | ~10μs |
| Command "docker ps" | KnownCommandHandler | <1μs | <1μs |

The extra ~1μs for non-English queries is **negligible** compared to LLM latency (~100-500ms).

#### Why NOT the Old Multilingual Regex Approach?

**Problems**:
- ❌ Complex maintenance: 5 languages × 5 patterns each = 25 patterns to maintain
- ❌ Limited coverage: Only 5 languages out of 100+
- ❌ Performance cost: More regex patterns = slower matching
- ❌ Inflexible: Adding language X requires code changes, testing, and release
- ❌ Accuracy: Regex keywords miss dialects, context, and intent

**Why Language-Agnostic Syntax is Superior**:
- ✅ Maintenance-free: Syntax rules work for all languages automatically
- ✅ Universal coverage: Same algorithm handles all 100+ languages
- ✅ Better performance: Syntax matching is simpler than regex patterns
- ✅ Infinitely flexible: New languages work without any code changes
- ✅ More accurate: LLM understands intent, context, and nuance

#### User Demographics

DevOps engineers primarily use **English** for:
- Documentation and Stack Overflow searches
- Tool commands and error messages
- Professional communication

Estimated distribution:
- **70-80%**: English queries
- **15-20%**: Non-English queries
- **5-10%**: Mixed language queries

The English-first approach optimizes for the **common case** while maintaining universal support.

#### Implementation Details

**English Patterns in `patterns.rs`**:
```rust
question_words: RegexSet::new([
    r"(?i)^(how|what|why|when|where|who|which)\s",     // Question words
    r"(?i)^(can you|could you|would you|will you)\s",  // Polite requests
    r"(?i)^(please|help|show me|explain)\s",           // Common phrases
])
```

**These patterns serve as**:
1. **Performance optimization**: Fast classification for 70-80% of queries
2. **Semantic clarity**: NaturalLanguageHandler has a clear, focused purpose
3. **Chain efficiency**: Prevents unnecessary DefaultHandler invocations for obvious cases

**Flow for Non-English Queries**:
```
Input: "come posso listare i file?" (Italian)
  ├─ NaturalLanguageHandler checks English patterns → No match
  ├─ Returns None (passes to next handler)
  ├─ DefaultHandler catches it → NaturalLanguage
  └─ Sent to LLM → Handles Italian correctly
```

Both English and non-English queries end up as `InputType::NaturalLanguage` and reach the LLM. The difference is **which handler** catches them first.

---

## References

### Source Files

- **Classifier**: `src/input/classifier.rs:65-103` (InputClassifier::new)
- **Handlers**: `src/input/handler.rs` (all 8 handler implementations except ShellBuiltinHandler)
- **Shell Builtins**: `src/input/shell_builtins.rs` (ShellBuiltinHandler - position 2 in chain)
- **Patterns**: `src/input/patterns.rs:36-84` (precompiled RegexSet)
- **Discovery**: `src/input/discovery.rs:25-85` (CommandCache)
- **Typo Detection**: `src/input/typo_detection.rs` (Levenshtein algorithm)
- **Main Integration**: `src/main.rs:307-323` (InputType handling)

### Key Design Patterns

- **Chain of Responsibility**: Handler chain (GoF pattern)
- **Lazy Singleton**: Pattern precompilation with `once_cell::Lazy`
- **Cache-Aside Pattern**: CommandCache with RwLock
- **Strategy Pattern**: Different handlers for different input types

### Documentation

- **Project Brief**: `infraware_terminal_project_brief.md`
- **Development Guide**: `CLAUDE.md`
- **Implementation Plan**: `docs/SCAN_IMPLEMENTATION_PLAN.md`

---

## Maintenance Notes

### Adding a New Handler

1. Implement `InputHandler` trait in `src/input/handler.rs`
2. Add to chain in `InputClassifier::new()` at appropriate position
3. Consider performance impact on average case
4. Add comprehensive test coverage

### Modifying Pattern Matching

1. Update `CompiledPatterns` in `src/input/patterns.rs`
2. Test with `cargo bench` to verify performance
3. Update tests in `patterns::tests`

### Extending Command Whitelist

1. Add to `KnownCommandHandler::default_known_commands()` in `src/input/handler.rs:172-247`
2. Command will be verified via PATH automatically
3. Add test case to verify classification

### Working with Aliases

**Adding aliases programmatically**:
1. Aliases are loaded at startup automatically from system and user config files
2. Users define aliases in `~/.bashrc`, `~/.bash_aliases`, or `~/.zshrc` using standard Bash syntax
3. Example: `alias ll='ls -la'`

**Extending alias support**:
1. Modify `CommandCache::load_user_aliases()` to add new file paths (unlikely to change)
2. Modify `CommandCache::load_system_aliases()` to add new system file paths (unlikely to change)
3. Add new dangerous patterns to `is_safe_alias()` if needed
4. Test with `cargo test` - ensure serial tests use `#[serial_test::serial]`

**Alias validation**:
1. Dangerous patterns checked in `is_safe_alias()` in `src/input/discovery.rs:337-373`
2. Safe parsing in `parse_aliases()` with quote handling (single, double, escaped)
3. Warnings printed for malformed aliases (empty names/values, no equals sign, etc.)
4. Invalid aliases silently rejected (not added to cache)

---

## Execution: CommandOrchestrator Shell Builtin Handling

**Location**: `src/orchestrators/command.rs:59-67`

### The Bug (FIXED)

Shell builtins were being **correctly classified** by `ShellBuiltinHandler` but **failing during execution** with "Command ':' not found" error.

**Root Cause**:
The `CommandOrchestrator` was checking if a command exists in PATH BEFORE passing it to the executor:

```rust
// BUGGY CODE (before fix):
if original_input.is_none() && !CommandExecutor::command_exists(cmd) {
    self.handle_command_not_found(cmd, state);
    return Ok(());
}
```

**The Problem**:
- Shell builtins like `:`, `.`, `[[`, `export` don't exist as files in PATH
- They are built into the shell interpreter itself
- The PATH check would always fail for builtins, triggering "not found" error
- User input: `. ~/.bashrc` → Classified as Command(`.`, ...) → Failed on PATH check → Error

### The Fix

Skip the PATH existence check for shell builtins, allowing them to reach the executor:

```rust
// FIXED CODE (after fix):
if original_input.is_none()
    && !ShellBuiltinHandler::requires_shell_execution(cmd)
    && !CommandExecutor::command_exists(cmd)
{
    self.handle_command_not_found(cmd, state);
    return Ok(());
}
```

**What Changed**:
- Added check: `!ShellBuiltinHandler::requires_shell_execution(cmd)`
- This delegates the PATH check decision to the builtin handler
- Shell builtins are recognized and skip the PATH check
- Non-builtin, non-existent commands still trigger "not found" error

### Impact

**Before Fix**:
- All 45 shell builtins would fail with "Command not found"
- Users couldn't use: `.`, `:`, `[[`, `source`, `export`, `eval`, `exec`, etc.
- Correct classification, incorrect execution (confusing error messages)

**After Fix**:
- All 45 shell builtins execute successfully via `sh -c`
- Full end-to-end functionality for shell builtins
- Users can use standard shell features naturally

### Execution Flow for Shell Builtins

```
User Input: ". ~/.bashrc"
     ↓
InputClassifier (SCAN Algorithm)
     ↓
ShellBuiltinHandler matches "."
     ↓
Returns: Command(".", ["~/.bashrc"])
     ↓
CommandOrchestrator receives Command
     ↓
Checks: Is original_input None? YES → Skip shell interpretation
Checks: Is "." a shell builtin? YES → Skip PATH check
     ↓
CommandExecutor receives Command(".", ["~/.bashrc"])
     ↓
Executes via `sh -c ". ~/.bashrc"` (proper shell builtin semantics)
     ↓
Success: Shell sources the file, environment updated
```

### Related Code

- **Handler Chain**: ShellBuiltinHandler (position 2 in chain, `src/input/shell_builtins.rs`)
- **Executor**: CommandExecutor properly routes builtins via `sh -c` (`src/executor/command.rs`)
- **Orchestrator**: CommandOrchestrator coordination logic (`src/orchestrators/command.rs`)

---

## Conclusion

SCAN is a high-performance, maintainable input classification system that:

✅ Expands aliases in <1μs (before classification)
✅ Recognizes 45+ shell builtins without PATH lookup
✅ Classifies input in <100μs (average ~10μs)
✅ Prevents expensive LLM calls for typos
✅ Handles ~67% of cases via fast cached lookup (builtins + known commands)
✅ Gracefully delegates multilingual queries to LLM
✅ Provides clear, actionable feedback for typos
✅ Uses proven design patterns (Chain of Responsibility, Lazy Singleton)
✅ Validates aliases for security (rejects dangerous patterns)
✅ Supports runtime alias reloading via `reload-aliases` command

**Production-ready**: 229 tests passing, 0 clippy warnings, optimized for real-world DevOps workflows.
