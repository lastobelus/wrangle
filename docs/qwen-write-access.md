# Qwen Write Access Through `wrangle`

This note documents how to run Qwen through `wrangle` in a way that actually lets it write files in the working directory.

## Short answer

If you want Qwen to modify files through `wrangle`, use Qwen with:

- the correct working directory
- a prompt that explicitly tells it which files to change
- `--permission-policy bypass`
- `--inherit-env` if Qwen auth is supplied by your shell environment

Example:

```bash
wrangle \
  --backend qwen \
  --permission-policy bypass \
  --inherit-env \
  "Edit README.md and docs/qwen-write-access.md to document how Qwen file writes work through wrangle. Only change those files." \
  /absolute/path/to/repo
```

Without `--permission-policy bypass`, `wrangle` currently invokes Qwen in its default approval mode, and headless Qwen will not auto-approve file edits.

If direct `qwen` works on your machine because it picks up auth from the shell environment, include `--inherit-env` when you run through `wrangle` too.

## What `wrangle` does today

`wrangle`'s Qwen adapter currently supports these permission policies:

- `default`
- `bypass`

In code, `bypass` maps to Qwen's `-y` flag:

- [`crates/wrangle-backends-cli/src/lib.rs`](/Users/lasto/projects/wrangle/crates/wrangle-backends-cli/src/lib.rs)

That means a dry run for a normal Qwen request looks like:

```json
{
  "program": "qwen",
  "args": ["-o", "stream-json", "...prompt..."],
  "currentDir": "..."
}
```

And a bypass request adds:

```json
{
  "program": "qwen",
  "args": ["-o", "stream-json", "-y", "...prompt..."],
  "currentDir": "..."
}
```

`wrangle` also sets the subprocess current directory to the request `work_dir`, so Qwen runs inside the target workspace rather than needing a separate `cd`.

## Confirmed working round trip

With real outbound network access enabled for the session, I verified an end-to-end file write through `wrangle` against a throwaway workspace under `tmp/`.

Command used:

```bash
cargo run -q -p wrangle-cli -- \
  --backend qwen \
  --permission-policy bypass \
  --inherit-env \
  "overwrite probe.txt with the single word changed" \
  /Users/lasto/projects/wrangle/tmp/qwen-write-probe
```

Observed result:

- `wrangle` launched Qwen with `permission_mode: "yolo"`
- Qwen emitted a `write_file` tool call for `probe.txt`
- `probe.txt` changed from `original` to `changed`

So the currently confirmed working recipe is:

- run Qwen through `wrangle`
- set `--permission-policy bypass`
- use `--inherit-env` when auth is coming from the environment
- give Qwen a concrete, scoped edit prompt

## Why prompt wording alone is not enough

I tested Qwen locally in headless `stream-json` mode against a throwaway workspace under `tmp/`.

Observed behavior before the model even answered:

- `default` mode exposed read-only planning tools plus `ask_user_question`, but no `edit`, `write_file`, or `run_shell_command`
- `--approval-mode auto-edit` exposed `edit` and `write_file`, but not `run_shell_command`
- `-y` / YOLO exposed `edit`, `write_file`, and `run_shell_command`

So the gating factor is permission mode, not just the natural-language prompt.

In other words:

- a strong prompt in `default` mode still does not give Qwen write tools
- the same prompt in an edit-capable mode does

## Recommended prompt shape

Once Qwen has an edit-capable mode, the best prompt is concrete and scoped:

- name the files or directories it may change
- say what result you want
- say what not to touch

Good example:

```text
Update README.md and docs/qwen-write-access.md to explain how Qwen writes files through wrangle. Keep the README note brief. Do not modify any other files.
```

Less reliable example:

```text
Please help with docs.
```

The permission mode unlocks the tools. The prompt tells Qwen how to use them safely.

## Safer mode Qwen supports, but `wrangle` does not expose yet

Qwen itself supports an intermediate mode:

- `--approval-mode auto-edit`

According to the Qwen docs, that mode auto-approves file edits while still requiring approval for shell commands. That is usually the safer choice for automated documentation or code edits.

Today, `wrangle` does not expose that mode for Qwen. The adapter only advertises `default` and `bypass`, and maps `bypass` to `-y`.

If `wrangle` later adds Qwen support for `PermissionPolicy::Auto`, it should probably map that to:

```text
qwen --approval-mode auto-edit
```

instead of forcing callers to choose between:

- `default`, which blocks unattended edits
- `bypass`, which also auto-approves shell commands

## One more practical wrinkle: auth and environment

During testing, direct `qwen -y ...` worked once the thread had real network access, but an early `wrangle` probe still failed with:

```text
No auth type is selected. Please configure an auth type (e.g. via settings or `--auth-type`) before running in non-interactive mode.
```

That failure was caused by invoking `wrangle` without the real shell environment that Qwen was using for auth. Re-running with `--inherit-env` fixed it.

Practical takeaway:

- if direct `qwen` works but `wrangle --backend qwen ...` does not
- and the failure mentions missing auth or auth type
- retry with `--inherit-env`

## Practical guidance

Use `default` when you want Qwen to inspect, plan, or summarize.

Use `bypass` when you need unattended edits through `wrangle` today and you trust the repo and prompt scope.

Prefer narrow prompts in `bypass` mode, for example:

- "Only update `README.md`."
- "Only edit files under `docs/`."
- "Do not run tests or install dependencies."

Those instructions do not replace the permission model, but they reduce the blast radius once write tools are available.

## Sources

- Qwen Code approval modes: [qwenlm.github.io/qwen-code-docs/en/users/features/approval-mode/](https://qwenlm.github.io/qwen-code-docs/en/users/features/approval-mode/)
- Qwen Code headless mode: [qwenlm.github.io/qwen-code-docs/en/users/features/headless/](https://qwenlm.github.io/qwen-code-docs/en/users/features/headless/)
