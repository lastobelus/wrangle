# wrangle

`wrangle` is a generalized agent execution and orchestration layer for local AI agents, with support for CLI-backed backends, API-backed adapters, persistent backend transport, and a first wrangle-owned server transport.

This repository began with an import of MIT-licensed code from [`localSummer/codeagent-wrapper-node`](https://github.com/localSummer/codeagent-wrapper-node), specifically the `codeagent-wrapper-rs` implementation. The imported code provided the starting point for the CLI wrapper runtime; `wrangle` now reshapes that code into a new public project with different interfaces, security defaults, and long-term architecture. See [docs/provenance.md](docs/provenance.md) for the full attribution trail.

For the product-level direction and issue framing, see [docs/roadmap.md](docs/roadmap.md).

## What v1 is for

- CLI-first execution of agent backends
- Normalized request, event, and result types
- A thin library API for programmatic callers such as orchestration systems and playbook runners
- Explicit permission-policy abstraction
- Transport separation from backend behavior
- Future-proofing for backend-owned persistence, project-local config, and a `wrangle`-native server

## Supported CLI backends

- Codex
- Claude
- Gemini
- Opencode
- Qwen

Qwen note: when invoked through `wrangle`, Qwen only gets file-write tools if the request uses an edit-capable approval mode. Today that means `--permission-policy bypass`, which `wrangle` maps to Qwen's `-y` / YOLO mode. If Qwen auth comes from the shell environment, also use `--inherit-env`. See [docs/qwen-write-access.md](docs/qwen-write-access.md).

## Supported API backends

- `codex-api`

## Transport model

`wrangle` distinguishes backend identity from transport identity:

- `OneShotProcess`: spawn a CLI for a single request
- `PersistentBackend`: attach to a backend-managed server when a backend supports it
- `WrangleServer`: a wrangle-owned long-lived local server transport

`wrangle` now implements all three transport identities, with `WrangleServer` as
the wrangle-owned long-lived attach point and `PersistentBackend` as the
backend-owned one.

## Current public entrypoints

- `wrangle backends --json`: inspect backend capabilities and availability
- `wrangle config-paths --json`: inspect project-local vs home-scoped config resolution
- `wrangle playbook land-work "..." --dry-run`: build the first playbook-oriented invocation path without executing it
- `wrangle-runner`: the library crate for programmatic callers that want to preview or execute requests without shelling out to the `wrangle` binary. See [docs/runner-api.md](docs/runner-api.md) for the supported API surface and examples.
- `plugins/wrangle-codex`: a repo-local Codex plugin for offloading tasks through `wrangle` with final-result-only handling. See [docs/plugins/wrangle-codex.md](docs/plugins/wrangle-codex.md).

## Security posture

`wrangle` is not a sandbox. It delegates execution trust to the selected backend executable and is careful about prompt logging, output buffering, and file-input gating. Read [SECURITY.md](SECURITY.md) and [docs/security-model.md](docs/security-model.md) before using it in automation.

If you are running `wrangle` inside another sandboxed host such as Codex, see [docs/troubleshooting.md](docs/troubleshooting.md) and [docs/project-config.md](docs/project-config.md).

## Workspace layout

- `crates/wrangle-core`: shared types and config model
- `crates/wrangle-transport`: transport contracts and execution transports
- `crates/wrangle-backends-api`: API-backed backend adapters
- `crates/wrangle-backends-cli`: CLI backend adapters
- `crates/wrangle-runner`: reusable execution API and playbook helpers for downstream orchestration callers
- `crates/wrangle-cli`: the `wrangle` binary
- `crates/wrangle-server`: protocol helpers for the wrangle-owned server transport
- `resources/codewrapper-agent-rs`: preserved upstream-derived import snapshot

## Status

This repo started CLI-first, but the internal interfaces now support:

- CLI-backed and API-backed execution through the same normalized runner surface
- project-local config discovery and configurable log placement
- backend-owned persistence and wrangle-owned server transport without redoing the request/result model
