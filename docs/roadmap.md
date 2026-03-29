# Wrangle Roadmap

## Big picture

`wrangle` is a reusable execution layer for agent backends.

It exists so higher-level systems can ask for agent work in a consistent way without needing to know:

- which backend is being used
- whether that backend is CLI-based or API-based
- how permission flags differ across vendors
- how sessions and transports are implemented underneath

The project is intentionally useful at two layers:

- as a CLI for direct human use
- as a library for other orchestration systems that need capability discovery, dry-run planning, execution, and playbook-shaped workflows

The first real consumers are not special to any one private project. In general, they look like:

- an orchestration system that needs to preview and execute agent work
- a playbook runner that needs a stable way to construct standard workflows such as `land-work`
- an inbox or task execution service that needs backend-agnostic execution and clear permission semantics

`wrangle` should let those systems stay backend-agnostic.

## Core product requirements

Any implementation work in `wrangle` should preserve these product constraints:

1. A caller should not need backend-specific knowledge for normal usage.
2. The same conceptual request should work across multiple backend implementations.
3. Capabilities must be inspectable programmatically.
4. Permission behavior must be explicit rather than implied.
5. Session and transport identity must be modeled clearly.
6. The library API should be a first-class integration surface, not an afterthought behind the CLI.
7. Future persistent transports and API backends should fit into the same core execution model.

## Current architecture direction

The architecture is intentionally split into:

- `wrangle-core`: shared execution types and config model
- `wrangle-backends-cli`: backend-specific CLI adapters
- `wrangle-transport`: transport implementation details
- `wrangle-runner`: programmatic execution API for downstream callers
- `wrangle-cli`: human-facing CLI

That split exists so downstream systems can integrate with `wrangle-runner` directly instead of scraping CLI output.

## Transport model

`wrangle` distinguishes backend identity from transport identity.

Current and planned transport modes:

- `OneShotProcess`
- `PersistentBackend`
- `WrangleServer`

This is important because a caller should be able to reason about:

- whether a request spawns a fresh process
- whether it attaches to a backend-owned server
- whether it attaches to a future wrangle-owned daemon

without changing the normalized request/result model.

## Roadmap order

The current recommended implementation order is:

1. Stabilize the runner API so downstream orchestration systems can use `wrangle` directly.
2. Expand permission policy and capability reporting so callers can make safe decisions without backend-specific branching.
3. Harden parallel execution before it becomes a trusted automation primitive.
4. Add Opencode persistent transport as the first real validation of the transport model beyond one-shot processes.
5. Add API-backed backends behind the same execution model.
6. Implement the native `WrangleServer` transport after the other abstractions have been exercised.

Steps 1 and 2 are complete. The permission model now supports four policies (`Default`, `Ask`, `Auto`, `Bypass`) with per-backend capability advertising and explicit rejection of unsupported combinations.

## What success looks like

A mature `wrangle` should make it possible for a downstream caller to:

- discover which backends are available and what they support
- preview an execution plan before running it
- execute a request through a stable library API
- request a standard playbook flow such as `land-work`
- choose transport and permission behavior explicitly
- remain mostly unchanged when the underlying backend changes

That is the standard each roadmap issue should be judged against.
