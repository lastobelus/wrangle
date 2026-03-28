# codeagent-wrapper-rs Security Review

## Scope

- Reviewed source copied to `resources/codewrapper-agent-rs`
- Focused on active runtime paths: CLI parsing, config loading, backend selection, process spawning, logging, parallel execution, and stream parsing
- Review date: 2026-03-27

## Method

- Manual static review of the Rust source and README/tests
- Looked specifically for command injection, arbitrary file access, secret leakage, unsafe defaults, and denial-of-service risks
- Dynamic verification was limited because `cargo` is not installed in this environment, so the findings below are based on source inspection rather than local execution

## Summary

The wrapper avoids classic shell injection because it uses `tokio::process::Command` with argument vectors rather than invoking a shell. That is a strong baseline.

The main security issues are in three areas:

1. Sensitive prompt content can be written to persistent logs.
2. Malicious or buggy backend output can drive unbounded memory growth.
3. Untrusted parallel task input can force arbitrary local file reads and exfiltrate their contents to a backend.

## Findings

### 1. Sensitive prompt and task data are logged to disk by default

- Severity: Medium
- Affected code:
  - `resources/codewrapper-agent-rs/src/executor.rs:151`
  - `resources/codewrapper-agent-rs/src/logger.rs:23`
  - `resources/codewrapper-agent-rs/src/parser.rs:49`
  - `resources/codewrapper-agent-rs/src/parser.rs:56`

`TaskExecutor::run()` logs the full backend argument list with `info!(args = ?args, ...)`. When the prompt is short enough to be passed as a positional CLI argument instead of stdin, that means the full user task is written into the log file under `~/.codeagent/logs/`. Session IDs and model identifiers are also logged there.

Separately, the parser emits full non-JSON lines and malformed JSON lines at `trace` level. If an operator enables a permissive `RUST_LOG` or similar tracing filter, backend output that includes secrets, source snippets, or tokens can also be persisted.

Impact:

- Secrets pasted into prompts can land in long-lived local logs.
- Prompts may contain proprietary source code, credentials, or customer data.
- The risk increases in shared machines, support environments, and when home directories are backed up or indexed.

Recommendation:

- Never log raw prompt/task content by default.
- Replace `args = ?args` with a redacted structure that omits the final prompt argument and masks session identifiers where possible.
- Avoid logging raw backend output lines; log only event metadata, lengths, or parse-failure counters.
- Consider making file logging opt-in rather than on by default for a wrapper that handles sensitive prompts.

### 2. Backend-controlled output can cause unbounded memory consumption

- Severity: High
- Affected code:
  - `resources/codewrapper-agent-rs/src/parser.rs:33`
  - `resources/codewrapper-agent-rs/src/parser.rs:36`
  - `resources/codewrapper-agent-rs/src/executor.rs:191`
  - `resources/codewrapper-agent-rs/src/executor.rs:194`
  - `resources/codewrapper-agent-rs/src/executor.rs:200`
  - `resources/codewrapper-agent-rs/src/executor.rs:211`

There are multiple independent memory-growth paths controlled by backend output:

- `JsonStreamParser` uses `read_line()` into a `String` and checks `MAX_MESSAGE_SIZE` only after the full line has already been read. A backend that emits a very large line without a newline can force a large allocation before the size check triggers.
- The stderr collector appends every line into a single `String` and never clears it inside the loop. That causes the entire stderr stream to be retained in memory.
- Parsed stdout events are appended to `events: Vec<serde_json::Value>` with no cap, so a backend can keep growing memory usage simply by emitting many valid JSON events.

Impact:

- A malicious backend binary, compromised CLI, or unexpectedly noisy backend can exhaust memory and crash the wrapper.
- This is especially relevant because the tool is meant to programmatically orchestrate agent CLIs, where long-running processes and large outputs are normal.

Recommendation:

- Enforce size limits before buffering entire lines, or switch to a framed reader with explicit maximum chunk sizes.
- Stream or truncate stderr instead of retaining the full buffer in memory.
- Bound the number or total size of retained events, or offer a mode that summarizes/streams events rather than storing all of them.
- Add regression tests that simulate oversized stdout lines, very large stderr, and long event streams.

### 3. Untrusted parallel task input can read arbitrary local files via `promptFile`

- Severity: Medium
- Affected code:
  - `resources/codewrapper-agent-rs/src/config.rs:170`
  - `resources/codewrapper-agent-rs/src/config.rs:137`
  - `resources/codewrapper-agent-rs/src/executor.rs:255`
  - `resources/codewrapper-agent-rs/src/executor.rs:256`
  - `resources/codewrapper-agent-rs/src/executor.rs:369`

Parallel mode accepts newline-delimited JSON task specs from stdin and deserializes `promptFile` directly into the runtime config. Later, `TaskExecutor::get_target()` reads that file path with `std::fs::read_to_string()` and uses its contents as the prompt sent to the selected backend.

There is no allowlist, path restriction, or trust boundary around `promptFile`. If a higher-level system feeds untrusted JSON into `codeagent --parallel`, an attacker can request sensitive local files such as shell configs, SSH material, project secrets, or API key files and have their contents forwarded to the backend process.

Impact:

- Local file disclosure to a backend or any substituted backend binary.
- Elevated risk when this wrapper is embedded into automation pipelines, task runners, or server-side orchestration that ingest externally supplied task definitions.

Recommendation:

- Treat parallel task specs as privileged input unless explicitly sandboxed.
- Restrict `promptFile` to an approved base directory, or require an explicit opt-in flag for file reads from task specs.
- Consider disallowing absolute paths and parent-directory traversal in task-fed prompt files.
- Document clearly that untrusted `--parallel` input is unsafe in the current design.

## Additional observations

### Whole-environment inheritance is a broad trust decision

- Relevant code:
  - `resources/codewrapper-agent-rs/src/executor.rs:66`
  - `resources/codewrapper-agent-rs/src/executor.rs:68`
  - `resources/codewrapper-agent-rs/src/executor.rs:161`

When `--minimal-env` is not set, the wrapper forwards the entire parent environment to the backend process. That is not automatically a vulnerability, but it does widen the blast radius if the selected backend executable is malicious or path-hijacked. In practice this can expose API keys, tokens, CI secrets, and unrelated credentials.

The current `--minimal-env` set still includes sensitive keys such as `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GOOGLE_API_KEY`, and `SSH_AUTH_SOCK`, so it is better described as reduced environment inheritance, not minimal trust.

### I did not find shell injection in command construction

- Relevant code:
  - `resources/codewrapper-agent-rs/src/executor.rs:161`
  - `resources/codewrapper-agent-rs/src/backend.rs:38`
  - `resources/codewrapper-agent-rs/src/backend.rs:82`
  - `resources/codewrapper-agent-rs/src/backend.rs:121`
  - `resources/codewrapper-agent-rs/src/backend.rs:155`

The wrapper builds argument vectors and executes them directly with `Command::new(...).args(...)`. That avoids the most common shell metacharacter injection class.

## Recommended remediation order

1. Stop logging raw prompt content and raw backend lines.
2. Add hard output-buffer limits for stdout events and stderr capture.
3. Gate or constrain `promptFile` when it comes from task specs.
4. Revisit environment inheritance and document the trust assumptions around backend executables and PATH resolution.

## Verdict

The project has a decent baseline against shell injection, but it is not ready to safely consume untrusted task definitions or backend output in its current form. The biggest practical risks are local secret leakage through logs and memory exhaustion from unbounded buffering.
