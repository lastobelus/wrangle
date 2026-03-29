# wrangle

`wrangle` is a generalized agent execution and orchestration layer for local AI agent CLIs, with a transport model that is ready for persistent backends and a future `wrangle` server.

This repository began with an import of MIT-licensed code from [`localSummer/codeagent-wrapper-node`](https://github.com/localSummer/codeagent-wrapper-node), specifically the `codeagent-wrapper-rs` implementation. The imported code provided the starting point for the CLI wrapper runtime; `wrangle` now reshapes that code into a new public project with different interfaces, security defaults, and long-term architecture. See [docs/provenance.md](docs/provenance.md) for the full attribution trail.

For the product-level direction and issue framing, see [docs/roadmap.md](docs/roadmap.md).

## What v1 is for

- CLI-first execution of agent backends
- Normalized request, event, and result types
- A thin library API for programmatic callers such as orchestration systems and playbook runners
- Explicit permission-policy abstraction
- Transport separation from backend behavior
- Future-proofing for Opencode persistent transport and a `wrangle`-native server

## Supported CLI backends

- Codex
- Claude
- Gemini
- Opencode
- Qwen

Qwen note: when invoked through `wrangle`, Qwen only gets file-write tools if the request uses an edit-capable approval mode. Today that means `--permission-policy bypass`, which `wrangle` maps to Qwen's `-y` / YOLO mode. If Qwen auth comes from the shell environment, also use `--inherit-env`. See [docs/qwen-write-access.md](docs/qwen-write-access.md).

## Transport model

`wrangle` distinguishes backend identity from transport identity:

- `OneShotProcess`: spawn a CLI for a single request
- `PersistentBackend`: attach to a backend-managed server when a backend supports it
- `WrangleServer`: reserved for a future native server transport

V1 implements `OneShotProcess` and keeps the interface stable for later persistent transports.

## Current public entrypoints

- `wrangle backends --json`: inspect backend capabilities and availability
- `wrangle playbook land-work "..." --dry-run`: build the first playbook-oriented invocation path without executing it
- `wrangle-runner`: the library crate for programmatic callers that want to preview or execute requests without shelling out to the `wrangle` binary. See [docs/runner-api.md](docs/runner-api.md) for the supported API surface and examples.

## Security posture

`wrangle` is not a sandbox. It delegates execution trust to the selected backend executable and is careful about prompt logging, output buffering, and file-input gating. Read [SECURITY.md](SECURITY.md) and [docs/security-model.md](docs/security-model.md) before using it in automation.

If you are running `wrangle` inside another sandboxed host such as Codex, see [docs/troubleshooting.md](docs/troubleshooting.md).

## Workspace layout

- `crates/wrangle-core`: shared types and config model
- `crates/wrangle-transport`: transport contracts and subprocess transport
- `crates/wrangle-backends-cli`: CLI backend adapters
- `crates/wrangle-runner`: reusable execution API and playbook helpers for downstream orchestration callers
- `crates/wrangle-cli`: the `wrangle` binary
- `resources/codewrapper-agent-rs`: preserved upstream-derived import snapshot

## Status

This repo is the initial public launch shape. It is intentionally CLI-first, but the internal interfaces are designed so that:

- Opencode can gain a persistent transport in v2 without changing the core request/result model
- `wrangle` can gain its own persistent server in v3 without redoing the transport boundary
