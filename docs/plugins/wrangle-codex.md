# `wrangle-codex` Plugin

`wrangle-codex` is a repo-local Codex plugin that offloads user requests through
`wrangle` to supported backends like `opencode`, `claude`, `gemini`, `qwen`,
and `codex`.

## Why it exists

The plugin gives Codex a consistent offload path without spending turns
narrating streaming subprocess output. It does that by invoking `wrangle` with:

- `--progress-file <path>`
- `--quiet-until-complete`

That lets `wrangle` write intermediate progress to a side-channel file while the
plugin waits for the final stdout result.

## Config

Repo-local config lives at `.codex/wrangle.toml`.

The most important setting is `wrangle_command`, which is an argv array so this
repo can dogfood the local dev build:

```toml
wrangle_command = ["cargo", "run", "-p", "wrangle-cli", "--"]
```

The config also supports:

- `timeout_secs`
- `default_permission_policy`
- `inherit_env`
- `pass_through_env`
- fixed `[env]`
- `backend_defaults.<backend>.model`

## Skill behavior

The plugin skill recognizes these forms:

- `use wrangle to tell opencode to ...`
- `[task]. Use wrangle`
- `tell claude to ...`
- `tell opencode with zai-coding-plan/glm-5.1 to ...`

If backend is omitted, the agent should infer it from recent conversation
context only when that is reliable. Otherwise it should ask one short
follow-up.

## Wrapper

The wrapper script is:

```bash
python3 plugins/wrangle-codex/scripts/run_wrangle.py --utterance "use wrangle to tell opencode to review this crate" --cwd "$PWD"
```

Use `--dry-run` to inspect parsing and command resolution:

```bash
python3 plugins/wrangle-codex/scripts/run_wrangle.py --utterance "tell claude to summarize the docs" --cwd "$PWD" --dry-run
```

The wrapper returns a normalized JSON summary with:

- resolved backend and model
- cwd and timeout
- `recommendedYieldTimeMs` for hosts that can block on one long-running command
- the exact `wrangle` argv
- success/failure
- final stdout result
- stderr excerpt on failure
- progress-file path and failure-tail context

## Final-result-only contract

The skill should:

- run one blocking wrapper command with a long timeout
- set the host command wait window to `recommendedYieldTimeMs` when possible
- avoid interactive polling
- avoid `write_stdin` or equivalent session polling unless the host unexpectedly returns control early
- emit no additional commentary after launch until completion or forced return
- avoid reporting streaming subprocess output
- only inspect the progress file for debugging or failure analysis

## Progress file event schema

Progress events are written as JSON Lines. Each line is a single JSON object.
All current events include a `type` field, and backend-emitted records also
include the resolved backend name.

Known event types today are:

| `type` | When written | Key fields |
|---|---|---|
| `lifecycle` | start, completion, or timeout | `state`, `backend`, `transport`, `workDir`, `hasSession`, `timeoutSecs`, `exitCode`, `durationMs` |
| `backendEvent` | each parsed stdout event from the backend | `backend`, `transport`, `payload` |
| `parseError` | when a backend stdout line cannot be parsed as JSON | `backend`, `message` |
| `stderrSummary` | after exit if the backend wrote to stderr | `backend`, `excerpt`, `truncated` |

Consumers should treat unknown event types as forward-compatible extensions
rather than errors.
