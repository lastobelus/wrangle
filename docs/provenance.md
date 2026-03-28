# Provenance

## Origin

`wrangle` began as an explicit import of code derived from the MIT-licensed `codeagent-wrapper-rs` implementation in:

- <https://github.com/localSummer/codeagent-wrapper-node>

Imported source snapshot in this repository:

- `resources/codewrapper-agent-rs`

Imported on:

- 2026-03-27

## What was adopted initially

The imported code provided the initial basis for:

- CLI argument handling ideas
- backend-specific command construction
- JSON event parsing
- subprocess execution flow
- session resume mechanics

## What changed in `wrangle`

After import, `wrangle` was restructured into a new public workspace with:

- new crate boundaries
- new public naming and branding
- explicit permission-policy abstractions
- transport-first architecture
- room for backend-managed persistent transports
- room for a future `wrangle`-native server
- tighter security defaults around logging, buffering, and prompt-file handling

## Attribution stance

This repository is independently maintained and may diverge substantially from the imported upstream basis over time. The upstream-derived code remains attributable to its original authors and contributors, and this repository keeps that attribution intentionally.

