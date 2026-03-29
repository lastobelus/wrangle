# Troubleshooting `wrangle` In Sandboxed Hosts

This document focuses on the practical problems you hit when running `wrangle` inside another tool that already imposes its own sandbox, trust model, or writable-root restrictions.

## Codex setup

If you want to run `wrangle` from Codex in `workspace-write` mode, the minimum reliable setup is:

1. Mark the project as trusted in Codex so project-local `.codex/config.toml` is applied.
2. Add a project-local Codex config that enables network access when your backend needs it.
3. Add `wrangle`'s home-scoped state directory to Codex writable roots.

Example:

```toml
# .codex/config.toml
[sandbox_workspace_write]
network_access = true
writable_roots = ["/Users/lasto/.wrangle"]
```

Replace `/Users/lasto` with your own home directory.

Why this is needed:

- `wrangle` writes CLI logs under `~/.wrangle/logs`
- `wrangle` loads agent/model config from `~/.wrangle/models.json`
- Codex `workspace-write` mode does not automatically allow writes to your home directory

If the project is not trusted, Codex may ignore the repo-local `.codex/config.toml`, which makes the writable-root change look ineffective.

## Common Codex symptoms

### `wrangle` panics creating its log file

Typical error:

```text
failed to create initial log file
```

Cause:

- Codex can write inside the repo, but not to `~/.wrangle/logs`

Fix:

- add `~/.wrangle` to `sandbox_workspace_write.writable_roots`

### Backend can reach the repo but cannot authenticate

Typical symptom:

- direct backend CLI works
- the same backend launched through `wrangle` fails due to missing auth or missing auth type

Cause:

- `wrangle` does not inherit the full shell environment unless you ask it to

Fix:

- run `wrangle` with `--inherit-env`

This matters most when backend auth is provided by environment variables or a shell-dependent setup rather than only by on-disk config.

### Project config changes do not seem to apply

Cause:

- the project is not trusted in Codex
- or the thread was started before the config state changed

Fix:

1. trust the project
2. confirm the repo-local `.codex/config.toml` is present
3. start a fresh thread if needed

## What `wrangle` currently stores under the home directory

Today `wrangle` has a meaningful home-directory dependency:

- logs: `~/.wrangle/logs`
- agent/model config: `~/.wrangle/models.json`

That is why sandboxed hosts need writable access outside the project root even when the target work itself is entirely inside the repo.

Tracked follow-up work:

- [#8 Support project-local wrangle config discovery](https://github.com/lastobelus/wrangle/issues/8)
- [#9 Make wrangle log directory configurable](https://github.com/lastobelus/wrangle/issues/9)

## Backend-specific notes

### Codex

`wrangle` currently runs Codex with `codex exec` and uses Codex's own sandbox and approval flags.

Operational notes:

- Codex supports `-C/--cd` for the working root
- Codex supports `--add-dir` for additional writable directories
- Codex project config can live in `.codex/config.toml`
- Codex user config lives in `~/.codex/config.toml`

If you are running `wrangle` inside Codex, remember you have two layers of permissions:

- Codex's sandbox controlling `wrangle`
- the backend CLI that `wrangle` launches

### Claude

Claude Code has good built-in project-local configuration support already.

Operational notes:

- project settings can live in `.claude/settings.json`
- local uncommitted settings can live in `.claude/settings.local.json`
- extra directories can be allowed with `--add-dir`
- permission behavior can be set with `--permission-mode`
- full bypass is `--dangerously-skip-permissions`

For `wrangle` callers, Claude is comparatively easy to tune per project because its config model is already project-aware.

### Gemini

Gemini CLI also has project-local settings, but trust is a first-class gate.

Operational notes:

- user settings live in `~/.gemini/settings.json`
- project settings live in `.gemini/settings.json`
- trusted-folder state is stored in `~/.gemini/trustedFolders.json`
- shell history is stored in `~/.gemini/tmp/<project_hash>/shell_history`
- extra directories can be added with `--include-directories`
- headless auto-approval can be enabled with `--yolo` or `--approval-mode`

Important trust behavior:

- if a folder is untrusted, Gemini ignores project `.gemini/settings.json`
- it also ignores project `.env` files and disables tool auto-acceptance

That makes Gemini's trust model similar in spirit to what we observed in Codex.

### Opencode

Opencode already supports project-local config discovery and upward traversal.

Operational notes:

- global config lives in `~/.config/opencode/opencode.json`
- project config can live in `opencode.json` at the project root
- Opencode searches upward to the nearest Git directory for project config
- provider credentials are stored in `~/.local/share/opencode/auth.json`
- permissions are controlled through the `permission` config
- access outside the working directory is controlled through `permission.external_directory`

Because Opencode already supports project config, it is a useful reference point for the `wrangle` issues in [#8](https://github.com/lastobelus/wrangle/issues/8) and [#9](https://github.com/lastobelus/wrangle/issues/9).

### Qwen

Qwen supports both project-level config and multiple approval modes, but `wrangle` does not yet expose the full Qwen permission model.

Operational notes:

- project settings can live in `.qwen/settings.json`
- user settings live in `~/.qwen/settings.json`
- headless session data is stored under `~/.qwen/projects/<sanitized-cwd>/chats`
- `--approval-mode auto-edit` auto-approves edits but still asks before shell commands
- `-y` / `--approval-mode yolo` auto-approves both edits and shell commands

Current `wrangle` note:

- for Qwen, `wrangle --permission-policy bypass` maps to Qwen `-y`
- `wrangle` does not yet expose Qwen `auto-edit`
- if Qwen auth depends on environment variables, use `--inherit-env`

See [docs/qwen-write-access.md](docs/qwen-write-access.md) for the verified working invocation.

## Sources

- OpenAI Codex CLI help output and local project testing
- Anthropic Claude Code settings: [docs.anthropic.com/en/docs/claude-code/settings](https://docs.anthropic.com/en/docs/claude-code/settings)
- Anthropic Claude Code CLI reference: [docs.anthropic.com/en/docs/claude-code/cli-reference](https://docs.anthropic.com/en/docs/claude-code/cli-reference)
- Gemini CLI trusted folders: [google-gemini.github.io/gemini-cli/docs/cli/trusted-folders.html](https://google-gemini.github.io/gemini-cli/docs/cli/trusted-folders.html)
- Gemini CLI configuration: [google-gemini.github.io/gemini-cli/docs/get-started/configuration.html](https://google-gemini.github.io/gemini-cli/docs/get-started/configuration.html)
- Gemini CLI headless mode: [google-gemini.github.io/gemini-cli/docs/cli/headless.html](https://google-gemini.github.io/gemini-cli/docs/cli/headless.html)
- OpenCode config: [opencode.ai/docs/config](https://opencode.ai/docs/config)
- OpenCode permissions: [opencode.ai/docs/permissions](https://opencode.ai/docs/permissions)
- OpenCode providers: [opencode.ai/docs/providers](https://opencode.ai/docs/providers)
- Qwen approval modes: [qwenlm.github.io/qwen-code-docs/en/users/features/approval-mode/](https://qwenlm.github.io/qwen-code-docs/en/users/features/approval-mode/)
- Qwen headless mode: [qwenlm.github.io/qwen-code-docs/en/users/features/headless/](https://qwenlm.github.io/qwen-code-docs/en/users/features/headless/)
- Qwen configuration: [qwenlm.github.io/qwen-code-docs/en/users/configuration/settings/](https://qwenlm.github.io/qwen-code-docs/en/users/configuration/settings/)
