---
name: use-wrangle
description: Offload a task through wrangle to a supported backend and only process the final result.
---

# Wrangle Offload

## Purpose

Use this skill when the user wants Codex to delegate work through `wrangle` to a
supported backend such as `opencode`, `claude`, `gemini`, `qwen`, or `codex`.

Supported trigger shapes include:

- `use wrangle to tell opencode to ...`
- `[task description]. Use wrangle`
- `tell [backend] to ...`
- `tell [backend] with [model] to ...`

## Rules

- Prefer one blocking wrapper invocation with a long timeout.
- If the host supports a command wait window such as `yield_time_ms`, set it to the full wrangle timeout window instead of polling.
- Do not open an interactive subprocess session.
- Do not poll just to narrate progress.
- Do not call `write_stdin` or an equivalent session-polling primitive unless the command unexpectedly returns control before the timeout window.
- Do not spend turns reporting streaming output from `wrangle`.
- After launch, send no additional commentary until the wrapper exits or the host forcibly returns control.
- Wait for completion, then summarize only the final result.
- If backend is omitted, infer it from recent conversation context only when that is reliable.
- If backend cannot be inferred safely, ask one short follow-up question.

## Command

Run the wrapper from the repo root:

```bash
python3 plugins/wrangle-codex/scripts/run_wrangle.py \
  --utterance "<original user request>" \
  --cwd "$PWD"
```

If you can reliably infer a prior backend from conversation context, pass it:

```bash
python3 plugins/wrangle-codex/scripts/run_wrangle.py \
  --utterance "<original user request>" \
  --cwd "$PWD" \
  --last-backend opencode
```

Use `--dry-run` when debugging parsing or config resolution:

```bash
python3 plugins/wrangle-codex/scripts/run_wrangle.py \
  --utterance "<original user request>" \
  --cwd "$PWD" \
  --dry-run
```

## Result Handling

- The wrapper launches `wrangle` with `--progress-file` and `--quiet-until-complete`.
- The wrapper also reports `recommendedYieldTimeMs`; when available, use that full wait window for the single blocking command.
- Intermediate backend events go to the progress file, not the user-facing response path.
- Normal success/failure handling should come from the wrapper's final JSON summary.
- Only inspect the progress file if the run fails or if you need extra debugging context.

## Output Expectations

- Report the backend and model that were actually used.
- Summarize the final result concisely.
- On failure, report the actionable stderr excerpt and the exit code.
- Do not quote or paraphrase intermediate progress lines unless explicitly debugging.
