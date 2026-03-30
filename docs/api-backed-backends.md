# API-Backed Backends

`wrangle` now supports API-backed backends alongside the existing CLI adapters.

## First API backend

The first representative adapter is:

- `codex-api`

It uses the OpenAI-compatible chat completions API and is surfaced through the
same runner request/result model as CLI-backed execution.

## Capability reporting

`wrangle backends --json` now includes an `implementation` field:

- `cli`
- `api`

This lets downstream callers distinguish process-backed and API-backed
execution without changing the top-level planning or result concepts they use.

## Current differences from CLI-backed execution

API-backed execution does not currently support:

- backend-native resumable sessions
- persistent backend transport
- non-default permission policies

So the current `codex-api` capability shape is:

- `implementation: "api"`
- `transportModes: ["oneShotProcess", "wrangleServer"]`
- `supportsResume: false`
- `supportedPermissionPolicies: ["default"]`

## Environment

`codex-api` requires:

- `OPENAI_API_KEY`

Optional:

- `OPENAI_BASE_URL`

## Preview and execution

Programmatic callers still use:

- `preview_request(...)`
- `execute_request(...)`
- `available_backends()`

The preview command is intentionally pseudo-command shaped so callers can keep
using the same dry-run and inspection concepts even when no subprocess is
involved.
