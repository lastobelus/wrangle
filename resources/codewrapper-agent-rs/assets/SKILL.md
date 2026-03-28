---
name: codeagent
description: Execute codeagent-wrapper for multi-backend AI code tasks. Supports Codex, Claude, Gemini, and Opencode backends with file references (@syntax) and structured output.
---

# Codeagent Wrapper Integration

## Overview

Execute codeagent-wrapper commands with pluggable AI backends (Codex, Claude, Gemini, Opencode). Supports file references via `@` syntax, parallel task execution with backend selection, agent presets from `~/.codeagent/models.json`, and configurable security controls.

This document applies to the Rust implementation (`codeagent-wrapper`).

## When to Use

- Complex code analysis requiring deep understanding
- Large-scale refactoring across multiple files
- Automated code generation with backend selection

## Quick Reference

Run `codeagent-wrapper --help` to see all available options.

**CLI Usage Pattern**:

```
codeagent-wrapper [options] <task> [workdir]
```

When using stdin (`-`), you can still pass an optional working directory as the second positional argument:

```bash
echo "run tests" | codeagent-wrapper - /path/to/workdir
```

## Usage

**HEREDOC with working directory** (recommended):

```bash
# Option 1: cd to target directory first (recommended)
cd /path/to/working/dir && codeagent-wrapper --backend codex - <<'__CODEAGENT_EOF__'
<task content here>
__CODEAGENT_EOF__

# Option 2: Use subshell to preserve current directory
(cd /path/to/working/dir && codeagent-wrapper --backend codex - <<'__CODEAGENT_EOF__'
<task content here>
__CODEAGENT_EOF__
)
```

**Simple tasks with working directory**:

```bash
codeagent-wrapper --backend codex "simple task" /path/to/working/dir
codeagent-wrapper --backend gemini "simple task" /path/to/working/dir
```

**Current directory** (no workdir needed):

```bash
codeagent-wrapper --backend claude - <<'__CODEAGENT_EOF__'
<task content here>
__CODEAGENT_EOF__
```

## Agent Presets

Use `-a, --agent <AGENT>` to apply a preset from `~/.codeagent/models.json`.

```bash
codeagent-wrapper --agent oracle "Analyze this repository"
codeagent-wrapper --agent develop "Implement feature X"
```

Built-in presets are used when `~/.codeagent/models.json` is missing:

| Agent                     | Backend  | Model               |
| ------------------------- | -------- | ------------------- |
| `oracle`                  | `claude` | `claude-sonnet-4-6` |
| `librarian`               | `claude` | `claude-sonnet-4-6` |
| `explore`                 | `codex`  | (unset)             |
| `develop`                 | `codex`  | (unset)             |
| `frontend-ui-ux-engineer` | `gemini` | (unset)             |
| `document-writer`         | `gemini` | (unset)             |

### Override Priority

When `--agent` is set:

1. Explicit CLI flags have highest priority (`--backend`, `--model`, `--prompt-file`, `--reasoning-effort`, `--skip-permissions`)
2. Agent preset values fill missing fields
3. Backend auto-detection is used only if backend is still unset

## Reliable HEREDOC Usage (script-safe)

Use a unique delimiter and keep it at column 1. This avoids "unexpected end of file" errors.

```bash
# Correct: cd first, then use stdin
cd /path/to/dir && codeagent-wrapper --backend codex - <<'__CODEAGENT_EOF__'
<task content here>
__CODEAGENT_EOF__
```

You can use `codeagent-wrapper - /path/to/dir` with stdin; `-` is task placeholder and `/path/to/dir` is workdir.

### Script checklist

- Ensure the script runs in bash: `#!/usr/bin/env bash`
- Delimiter must match exactly and be at line start (no spaces, no tabs)
- Do not use CRLF line endings (convert to LF if needed)
- If you must indent the body, use `<<-__CODEAGENT_EOF__` and only TABs for indentation
- Avoid using a delimiter that may appear alone in the content

### Common failure patterns

- `EOF` line has leading whitespace
- `EOF` line ends with hidden `\r` (Windows line endings)
- Unclosed quotes or brackets before the heredoc
- Running the script with `sh` when it uses bash features

## Backends

| Backend  | Command              | Parameters                       | Description             | Best For                             |
| -------- | -------------------- | -------------------------------- | ----------------------- | ------------------------------------ |
| codex    | `--backend codex`    | `--full-auto`                    | OpenAI Codex            | Code analysis, complex development   |
| claude   | `--backend claude`   | `--dangerously-skip-permissions` | Anthropic Claude        | Simple tasks, documentation, prompts |
| gemini   | `--backend gemini`   | `--yolo`                         | Google Gemini           | UI/UX prototyping                    |
| opencode | `--backend opencode` | -                                | Opencode (MiniMax-M2.1) | Code exploration                     |

### Backend Selection Guide

**Codex**:

- Deep code understanding and complex logic implementation
- Large-scale refactoring with precise dependency tracking
- Algorithm optimization and performance tuning
- Example: "Analyze the call graph of @src/core and refactor the module dependency structure"

**Claude**:

- Quick feature implementation with clear requirements
- Technical documentation, API specs, README generation
- Professional prompt engineering (e.g., product requirements, design specs)
- Example: "Generate a comprehensive README for @package.json with installation, usage, and API docs"

**Gemini**:

- UI component scaffolding and layout prototyping
- Design system implementation with style consistency
- Interactive element generation with accessibility support
- Example: "Create a responsive dashboard layout with sidebar navigation and data visualization cards"

**Backend Switching**:

- Start with Codex for analysis, switch to Claude for documentation, then Gemini for UI implementation
- Use per-task backend selection in parallel mode to optimize for each task's strengths

**Opencode**:

- Quick code exploration and navigation
- MiniMax-M2.1 powered chat for understanding existing codebases (default model: minimax/MiniMax-M2.1)
- Example: "Explore the architecture of this project and identify key modules"

## Parameters

- `task` (required): Task description, supports `@file` references
- `working_dir` (optional): Working directory (default: current)
- `--backend` (optional): Select AI backend (codex/claude/gemini/opencode). If omitted, backend is auto-detected (Claude -> Codex -> Gemini -> Opencode)
- `--agent` / `-a` (optional): Apply agent preset from `~/.codeagent/models.json`
- `--model` (optional): Model override
- `--prompt-file` (optional): Read task content from file
- `--reasoning-effort` (optional): Reasoning effort level (backend/model dependent)
- `--skip-permissions` / `--yolo` (optional): Skip permission checks for Claude (`--dangerously-skip-permissions`) and Codex (`--full-auto`)
- `--timeout` (optional): Timeout in seconds (default: 7200)

## Return Format

```
Agent response text here...

---
SESSION_ID: 019a7247-ac9d-71f3-89e2-a823dbd8fd14
```

## Resume Session

```bash
# Resume with codex backend (in target directory)
cd /path/to/dir && codeagent-wrapper --backend codex resume <session_id> - <<'__CODEAGENT_EOF__'
<follow-up task>
__CODEAGENT_EOF__

# Resume with simple task string
codeagent-wrapper --backend claude resume <session_id> "follow-up task"
```

## Parallel Execution

Parallel mode reads **JSON Lines** from stdin (one task per line):

```bash
codeagent-wrapper --parallel <<'__CODEAGENT_EOF__'
{"id":"task1","task":"analyze code structure","workDir":"/path/to/dir","backend":"codex"}
{"id":"task2","task":"implement based on analysis","dependencies":["task1"],"agent":"develop"}
__CODEAGENT_EOF__
```

With per-task backend and dependencies:

```bash
codeagent-wrapper --parallel <<'__CODEAGENT_EOF__'
{"id":"task1","task":"analyze code structure","backend":"codex","workDir":"/path/to/dir"}
{"id":"task2","task":"design architecture based on analysis","backend":"claude","dependencies":["task1"]}
{"id":"task3","task":"generate implementation code","backend":"gemini","dependencies":["task2"]}
__CODEAGENT_EOF__
```

**Concurrency Control**:
Set `CODEAGENT_MAX_PARALLEL_WORKERS` to limit concurrent tasks (default: `min(100, cpuCount*4)`).

## Environment Variables

- `CODEX_TIMEOUT`: Override timeout in seconds (default: 7200 = 2 hours)
- `CODEAGENT_SKIP_PERMISSIONS`: Control permission checks
  - For **Claude** backend: Adds `--dangerously-skip-permissions` (default: disabled)
  - For **Codex** backend: Adds `--full-auto` (default: disabled)
  - For **Gemini/Opencode** backends: No effect
- `CODEAGENT_MAX_PARALLEL_WORKERS`: Limit concurrent tasks in parallel mode (default: min(100, cpuCount\*4), recommended: 8)

## Invocation Pattern

**Single Task with working directory**:

```
Bash tool parameters:
- command: cd /path/to/working/dir && codeagent-wrapper --backend <backend> - <<'__CODEAGENT_EOF__'
  <task content>
  __CODEAGENT_EOF__
- timeout: 7200000
- description: <brief description>

Note: --backend is optional. If omitted, backend is resolved by CLI/backend auto-detection rules.
```

**Single Task in current directory**:

```
Bash tool parameters:
- command: codeagent-wrapper --backend <backend> - <<'__CODEAGENT_EOF__'
  <task content>
  __CODEAGENT_EOF__
- timeout: 7200000
- description: <brief description>
```

**Parallel Tasks**:

```
Bash tool parameters:
- command: codeagent-wrapper --parallel <<'__CODEAGENT_EOF__'
  {"id":"task_id","task":"task content","backend":"<backend>","workDir":"/path","dependencies":["dep1","dep2"]}
  __CODEAGENT_EOF__
- timeout: 7200000
- description: <brief description>

Note: Per-task backend is optional; if omitted, CLI `--backend` is used, then agent preset, then backend auto-detection.
```

## Critical Rules

**NEVER kill codeagent processes.** Long-running tasks are normal. Instead:

1. **Check task status via log file**:

   ```bash
   # View real-time output
   tail -f /tmp/claude/<workdir>/tasks/<task_id>.output

   # Check if task is still running
   cat /tmp/claude/<workdir>/tasks/<task_id>.output | tail -50
   ```

2. **Wait with timeout**:

   ```bash
   # Use TaskOutput tool with block=true and timeout
   TaskOutput(task_id="<id>", block=true, timeout=300000)
   ```

3. **Check process without killing**:
   ```bash
   ps aux | grep codeagent-wrapper | grep -v grep
   ```

**Why:** codeagent tasks often take 2-10 minutes. Killing them wastes API costs and loses progress.

## Security Best Practices

- **Claude Backend**: Permission checks enabled by default
  - To skip checks: set `CODEAGENT_SKIP_PERMISSIONS=true` or pass `--skip-permissions` / `--yolo`
  - Both enable `--dangerously-skip-permissions` flag for Claude CLI
- **Codex Backend**: Use `--skip-permissions` / `CODEAGENT_SKIP_PERMISSIONS=true` to enable `--full-auto`
- **Concurrency Limits**: Set `CODEAGENT_MAX_PARALLEL_WORKERS` in production to prevent resource exhaustion
- **Automation Context**: This wrapper is designed for AI-driven automation where permission prompts would block execution

## Recent Updates

- Multi-backend support for all modes (workdir, resume, parallel): Codex, Claude, Gemini, Opencode
- Agent presets enabled in runtime via `--agent` / `-a` (single task + resume + parallel)
- `~/.codeagent/models.json` is the active config source; built-in defaults are used when missing
- Security controls with configurable permission checks (`--skip-permissions` / `--yolo`)
- Concurrency limits with adaptive worker pool (min(100, cpuCount\*4))
