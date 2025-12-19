# Supported Commands Reference

This document lists all commands supported by Infraware Terminal, organized by category, with references to the corresponding source files and tests.

## Command Categories Overview

| Category | Count | Source File |
|----------|-------|-------------|
| Application Builtins | 8 | `src/input/application_builtins.rs` |
| Shell Builtins | 40+ | `src/input/shell_builtins.rs` |
| Known DevOps Commands | 90+ | `src/input/known_commands.rs` |
| Interactive Commands (Supported) | 28 | `src/executor/command.rs` |
| Interactive Commands (Blocked) | 25 | `src/executor/command.rs` |
| Flood Protection | N/A | `src/executor/command.rs` |

---

## Flood Protection (Ctrl+C Handler)

The terminal implements comprehensive flood protection to ensure it **never blocks** and always responds to Ctrl+C.

### Protected Commands

| Command | Protection | Behavior |
|---------|------------|----------|
| `yes` | Output limited | Max 1000 lines, cancellable via Ctrl+C |
| `seq 1 999999999` | Output limited | Max 1000 lines, cancellable via Ctrl+C |
| `cat /dev/zero` | **Blocked** | Error: infinite device |
| `cat /dev/urandom` | **Blocked** | Error: infinite device |
| `cat /dev/random` | **Blocked** | Error: infinite device |
| `dd if=/dev/zero` | **Blocked** | Error: infinite device (unless `count=` specified) |
| `while true; do echo x; done` | Cancellable | Responds to Ctrl+C within 2 seconds |

### Background Flood Protection

Commands that produce infinite output are blocked from running in background:

| Command | Blocked |
|---------|---------|
| `yes &` | Yes - infinite output |
| `cat /dev/zero &` | Yes - infinite device |
| `cat /dev/urandom &` | Yes - infinite device |

### Allowed with Limits

| Command | Allowed |
|---------|---------|
| `cat /dev/urandom \| head -c 100` | Yes - output limited by pipe |
| `dd if=/dev/zero count=10` | Yes - has count limit |
| `ping -c 5 localhost` | Yes - has count limit |

### Ctrl+C (CancellationToken) Behavior

- **Immediate response**: All running commands respond to Ctrl+C
- **SIGINT propagation**: Signal sent to child processes
- **500ms grace period**: After SIGINT, waits 500ms before force-kill
- **No blocking**: Terminal UI remains responsive during command execution

### Flood Tests

Located in `tests/executor_tests.rs`:

| Test | Verifies |
|------|----------|
| `test_flood_yes_command_cancellable` | `yes` stops within 2s after Ctrl+C |
| `test_flood_seq_large_output_limited` | `seq` output truncated to ~1000 lines |
| `test_flood_seq_cancellable` | `seq` with huge range cancellable |
| `test_flood_background_yes_blocked` | `yes &` returns error |
| `test_flood_background_cat_dev_zero_blocked` | `cat /dev/zero &` returns error |
| `test_flood_background_cat_dev_urandom_blocked` | `cat /dev/urandom &` returns error |
| `test_flood_multiple_cancellation_tokens` | Multiple concurrent commands cancellable |
| `test_flood_rapid_cancellation` | Rapid cancel/execute cycles stable |
| `test_flood_output_streaming_cancellable` | Streaming output interruptible |
| `test_flood_shell_command_cancellable` | Shell infinite loops cancellable |

### Source Files

- **Implementation**: `src/executor/command.rs`
  - `INFINITE_DEVICES` - blocked device paths
  - `INFINITE_OUTPUT_COMMANDS` - commands blocked from background
  - `targets_infinite_device()` - detection function
  - `shell_command_has_infinite_device()` - shell bypass detection
- **Tests**: `tests/executor_tests.rs` (Flood Command Protection Tests section)

---

## 1. Application Builtins

Commands implemented directly in the terminal application. Handled by `ApplicationBuiltinHandler`.

| Command | Description | Test File |
|---------|-------------|-----------|
| `cd` | Change working directory | `tests/classifier_tests.rs` |
| `clear` | Clear terminal output buffer | `src/input/application_builtins.rs` |
| `exit` | Exit the terminal application | `src/input/application_builtins.rs` |
| `jobs` | List background jobs | `src/input/application_builtins.rs` |
| `history` | Show command history | `src/input/application_builtins.rs` |
| `reload-aliases` | Reload alias definitions | `src/input/application_builtins.rs` |
| `reload-commands` | Clear command cache | `src/input/application_builtins.rs` |
| `auth-status` | Check backend authentication | `src/input/application_builtins.rs` |

**Tests:**
- `test_clear_is_builtin`
- `test_reload_aliases_is_builtin`
- `test_reload_commands_is_builtin`
- `test_exit_is_builtin`
- `test_history_is_builtin`
- `test_jobs_is_builtin`
- `test_cd_is_builtin`
- `test_builtin_list_count`

---

## 2. Shell Builtins

POSIX/bash/zsh shell builtins that don't exist in PATH. Handled by `ShellBuiltinHandler`.

### Source Commands
| Command | Requires Shell | Unix Only |
|---------|---------------|-----------|
| `.` | Yes | Yes |
| `source` | Yes | Yes |

### Boolean/Test Commands
| Command | Requires Shell | Unix Only |
|---------|---------------|-----------|
| `:` | Yes | Yes |
| `true` | No | Yes |
| `false` | No | Yes |
| `[` | No | Yes |
| `[[` | Yes | Yes |
| `test` | No | Yes |

### Variable/Environment Commands
| Command | Requires Shell | Unix Only |
|---------|---------------|-----------|
| `export` | Yes | Yes |
| `unset` | Yes | Yes |
| `set` | Yes | Yes |
| `declare` | Yes | Yes |
| `local` | Yes | Yes |
| `readonly` | Yes | Yes |
| `typeset` | Yes | Yes |

### Evaluation/Execution Commands
| Command | Requires Shell | Unix Only |
|---------|---------------|-----------|
| `eval` | Yes | Yes |
| `exec` | Yes | Yes |
| `return` | Yes | Yes |

### Flow Control
| Command | Requires Shell | Unix Only |
|---------|---------------|-----------|
| `break` | Yes | Yes |
| `continue` | Yes | Yes |
| `shift` | Yes | Yes |

### Alias Management
| Command | Requires Shell | Unix Only |
|---------|---------------|-----------|
| `alias` | Yes | Yes |
| `unalias` | Yes | Yes |

### I/O Commands
| Command | Requires Shell | Unix Only |
|---------|---------------|-----------|
| `read` | Yes | Yes |
| `echo` | No | No |
| `printf` | No | Yes |

### Job Control
| Command | Requires Shell | Unix Only |
|---------|---------------|-----------|
| `jobs` | Yes | Yes |
| `fg` | Yes | Yes |
| `bg` | Yes | Yes |
| `wait` | Yes | Yes |

### Directory Stack
| Command | Requires Shell | Unix Only |
|---------|---------------|-----------|
| `pushd` | Yes | Yes |
| `popd` | Yes | Yes |
| `dirs` | Yes | Yes |

### Builtin Management
| Command | Requires Shell | Unix Only |
|---------|---------------|-----------|
| `builtin` | Yes | Yes |
| `command` | Yes | Yes |
| `enable` | Yes | Yes |

### System Info
| Command | Requires Shell | Unix Only |
|---------|---------------|-----------|
| `type` | Yes | Yes |
| `hash` | Yes | Yes |
| `times` | Yes | Yes |
| `umask` | Yes | Yes |
| `ulimit` | Yes | Yes |

**Tests:** (`src/input/shell_builtins.rs`)
- `test_recognize_dot_builtin`
- `test_recognize_dot_with_file`
- `test_recognize_colon_builtin`
- `test_recognize_single_bracket`
- `test_recognize_double_bracket`
- `test_recognize_test_command`
- `test_recognize_true_false`
- `test_recognize_source`
- `test_recognize_export`
- `test_not_builtin`
- `test_is_builtin_check`
- `test_preserve_shell_operators`

---

## 3. Known DevOps Commands

Commands recognized by `KnownCommandHandler` with PATH verification and caching.

### Basic Shell Commands
| Command | Description |
|---------|-------------|
| `ls` | List directory contents |
| `pwd` | Print working directory |
| `cat` | Concatenate and display files |
| `echo` | Display text |
| `grep` | Search text patterns |
| `find` | Search for files |
| `mkdir` | Create directories |
| `rm` | Remove files/directories |
| `cp` | Copy files |
| `mv` | Move/rename files |
| `touch` | Create empty files |
| `chmod` | Change file permissions |
| `chown` | Change file ownership |
| `ln` | Create links |
| `tar` | Archive files |
| `gzip` | Compress files |
| `gunzip` | Decompress files |
| `zip` | Create zip archives |
| `unzip` | Extract zip archives |
| `clear` | Clear terminal |

### Text Processing
| Command | Description |
|---------|-------------|
| `sed` | Stream editor |
| `awk` | Pattern scanning |
| `sort` | Sort lines |
| `uniq` | Filter duplicates |
| `wc` | Word count |
| `head` | Output first lines |
| `tail` | Output last lines |
| `cut` | Remove sections |
| `paste` | Merge lines |
| `tr` | Translate characters |

### Process Management
| Command | Description |
|---------|-------------|
| `ps` | Process status |
| `kill` | Send signal to process |
| `killall` | Kill by name |
| `pkill` | Kill by pattern |
| `jobs` | List jobs |
| `bg` | Background job |
| `fg` | Foreground job |

### Network Utilities
| Command | Description |
|---------|-------------|
| `curl` | Transfer data |
| `wget` | Download files |
| `ping` | Test connectivity |
| `netstat` | Network statistics |
| `ss` | Socket statistics |
| `ip` | IP configuration |
| `ifconfig` | Interface config |
| `dig` | DNS lookup |
| `nslookup` | DNS query |
| `traceroute` | Trace route |
| `ssh` | Secure shell |
| `scp` | Secure copy |
| `rsync` | Remote sync |

### System Information
| Command | Description |
|---------|-------------|
| `uname` | System info |
| `hostname` | Host name |
| `whoami` | Current user |
| `who` | Logged in users |
| `w` | User activity |
| `uptime` | System uptime |
| `free` | Memory usage |
| `df` | Disk space |
| `du` | Directory usage |

### Privilege Escalation
| Command | Description |
|---------|-------------|
| `sudo` | Execute as root |
| `su` | Switch user |

### Docker & Containers
| Command | Description |
|---------|-------------|
| `docker` | Container management |
| `docker-compose` | Multi-container apps |
| `docker-machine` | Docker machines |

### Kubernetes
| Command | Description |
|---------|-------------|
| `kubectl` | Kubernetes CLI |
| `helm` | Kubernetes packages |
| `minikube` | Local Kubernetes |
| `k9s` | Kubernetes TUI |

### Cloud Providers
| Command | Description |
|---------|-------------|
| `aws` | Amazon Web Services |
| `az` | Microsoft Azure |
| `gcloud` | Google Cloud |
| `terraform` | Infrastructure as Code |
| `terragrunt` | Terraform wrapper |
| `pulumi` | IaC platform |

### Version Control
| Command | Description |
|---------|-------------|
| `git` | Git VCS |
| `svn` | Subversion |
| `hg` | Mercurial |

### Build Tools
| Command | Description |
|---------|-------------|
| `make` | Build automation |
| `cmake` | Cross-platform build |
| `cargo` | Rust package manager |
| `npm` | Node.js packages |
| `yarn` | JavaScript packages |
| `pip` | Python packages |
| `pipenv` | Python environments |
| `poetry` | Python dependency |
| `maven` | Java builds |
| `gradle` | Java builds |
| `ant` | Java builds |

### Package Managers
| Command | Description |
|---------|-------------|
| `apt` | Debian/Ubuntu |
| `apt-get` | Debian/Ubuntu |
| `yum` | RHEL/CentOS |
| `dnf` | Fedora |
| `pacman` | Arch Linux |

### Monitoring
| Command | Description |
|---------|-------------|
| `prometheus` | Metrics |
| `grafana` | Visualization |
| `datadog` | Monitoring |

### DevOps Tools
| Command | Description |
|---------|-------------|
| `ansible` | Configuration management |
| `ansible-playbook` | Run playbooks |
| `vagrant` | VM management |
| `packer` | Image building |
| `consul` | Service mesh |
| `vault` | Secrets management |

**Tests:** (`src/input/known_commands.rs`)
- `test_default_commands_not_empty`
- `test_contains_docker_commands`
- `test_contains_kubernetes_commands`
- `test_contains_basic_shell_commands`
- `test_no_duplicates`

---

## 4. Interactive Commands (Supported)

Commands that suspend the TUI for full terminal access. Defined in `REQUIRES_INTERACTIVE`.

### Text Editors (7)
| Command | Description |
|---------|-------------|
| `vim` | Vi improved |
| `nvim` | Neovim |
| `nano` | Simple editor |
| `emacs` | Emacs editor |
| `pico` | Pine composer |
| `ed` | Line editor |
| `vi` | Vi editor |

### Pagers (5)
| Command | Description |
|---------|-------------|
| `less` | File pager |
| `more` | File pager |
| `most` | File pager |
| `man` | Manual pages |
| `info` | Info pages |

### File Managers (5)
| Command | Description |
|---------|-------------|
| `mc` | Midnight Commander |
| `ranger` | File manager |
| `nnn` | File manager |
| `lf` | File manager |
| `vifm` | Vi-like file manager |

### System Monitors (4)
| Command | Description |
|---------|-------------|
| `top` | Process viewer |
| `htop` | Interactive top |
| `btop` | Resource monitor |
| `atop` | Advanced top |

### Other Interactive (2)
| Command | Description |
|---------|-------------|
| `watch` | Execute periodically |
| `gh` | GitHub CLI |

**Tests:** (`tests/interactive_command_test.rs`, `tests/executor_tests.rs`)
- `test_requires_interactive`
- `test_vim_requires_interactive`
- `test_nano_requires_interactive`

---

## 5. Interactive Commands (Blocked)

Commands that are blocked with helpful error messages. Defined in `INTERACTIVE_BLOCKED`.

### Remote/Session (6)
| Command | Alternative |
|---------|-------------|
| `ssh` | Use external terminal |
| `telnet` | Use external terminal |
| `ftp` | Use external terminal |
| `sftp` | Use external terminal |
| `screen` | Use external terminal |
| `tmux` | Use external terminal |

### REPLs (10)
| Command | Alternative |
|---------|-------------|
| `python` | Use `python -c "..."` |
| `python3` | Use `python3 -c "..."` |
| `irb` | Use Ruby scripts |
| `node` | Use `node -e "..."` |
| `ipython` | Use scripts |
| `mysql` | Use connection flags |
| `psql` | Use connection flags |
| `sqlite3` | Use connection flags |
| `mongo` | Use connection flags |
| `redis-cli` | Use connection flags |

### Debuggers (3)
| Command | Alternative |
|---------|-------------|
| `gdb` | Use scripts |
| `lldb` | Use scripts |
| `pdb` | Use scripts |

### Terminal Browsers (3)
| Command | Alternative |
|---------|-------------|
| `w3m` | Use external browser |
| `lynx` | Use external browser |
| `links` | Use external browser |

### Admin Tools (2)
| Command | Alternative |
|---------|-------------|
| `passwd` | Use external terminal |
| `visudo` | Use external terminal |

### System Monitors (Root Required) (3)
| Command | Alternative |
|---------|-------------|
| `iotop` | Use `sudo iotop` in external terminal |
| `iftop` | Use `sudo iftop` in external terminal |
| `nethogs` | Use `sudo nethogs` in external terminal |

**Tests:** (`tests/executor_tests.rs`)
- `test_ssh_command_blocked`
- `test_python_command_blocked`

---

## 6. Interactive Subcommands (Blocked)

Specific command+subcommand combinations that are blocked (e.g., auth commands that open browser).

| Command | Subcommand | Reason |
|---------|------------|--------|
| `gcloud` | `auth` | Opens browser |
| `az` | `login` | Opens browser |
| `aws` | `sso` | Opens browser |
| `gh` | `auth` | Opens browser |
| `firebase` | `login` | Opens browser |
| `heroku` | `login` | Opens browser |
| `netlify` | `login` | Opens browser |
| `vercel` | `login` | Opens browser |

---

## Test Files Reference

| Test File | Purpose |
|-----------|---------|
| `tests/classifier_tests.rs` | SCAN algorithm, handler chain, input classification |
| `tests/executor_tests.rs` | Command execution, interactive commands, blocking |
| `tests/integration_tests.rs` | End-to-end workflows |
| `tests/interactive_command_test.rs` | TUI suspend/resume |
| `tests/terminal_state_tests.rs` | Terminal state management |
| `src/input/known_commands.rs` | Known commands unit tests |
| `src/input/shell_builtins.rs` | Shell builtins unit tests |
| `src/input/application_builtins.rs` | Application builtins unit tests |

---

## Running Tests

```bash
# All tests
cargo test

# Specific test file
cargo test --test classifier_tests
cargo test --test executor_tests
cargo test --test integration_tests
cargo test --test interactive_command_test

# Specific test
cargo test test_requires_interactive
cargo test test_recognize_dot_builtin

# With output
cargo test -- --nocapture
```

---

## Adding New Commands

### To Known Commands
1. Edit `src/input/known_commands.rs`
2. Add command to appropriate category in `default_devops_commands()`
3. Add test in the same file if needed

### To Shell Builtins
1. Edit `src/input/shell_builtins.rs`
2. Add `ShellBuiltinInfo` entry in `builtin_info()`
3. Add test for the new builtin

### To Application Builtins
1. Edit `src/input/application_builtins.rs`
2. Add to `APPLICATION_BUILTINS` constant
3. Implement handler in `main.rs` or orchestrator
4. Add test

### To Interactive Commands
1. Edit `src/executor/command.rs`
2. Add to `REQUIRES_INTERACTIVE` (supported) or `INTERACTIVE_BLOCKED` (blocked)
3. Add test in `tests/executor_tests.rs`
