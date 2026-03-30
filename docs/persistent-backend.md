# Persistent Backend Transport

## Overview

The `PersistentBackend` transport mode enables wrangle to connect to a backend's
long-lived server process instead of spawning a fresh one-shot process for each
execution. For Opencode, this uses the `--server` flag to attach to or start a
persistent server instance.

## Transport identity

wrangle models three transport identities:

| Transport | Description |
|---|---|
| `OneShotProcess` | Spawns a fresh backend process per execution |
| `PersistentBackend` | Attaches to a backend-owned persistent server |
| `WrangleServer` | Reserved for future wrangle-owned daemon |

Downstream callers should use `ExecutionRequest`, `ExecutionResult`, and
`SessionHandle` the same way regardless of transport mode. The normalized model
is stable across all three.

## Lifecycle differences

### One-shot (default)

1. wrangle spawns a new backend process
2. Backend runs the task and exits
3. wrangle collects exit code, events, and session id
4. Session state is `Resumable` (can be re-attached via `-r`)

### Persistent backend

1. wrangle invokes the backend with `--server` flag
2. Backend starts or attaches to its persistent server
3. Task runs through the persistent server
4. wrangle collects results the same way
5. Session state is `PersistentAttached`
6. If persistent execution fails, wrangle falls back to one-shot automatically

## Capability detection

Call `wrangle backends` or `wrangle backends --json` to see which backends
support persistent mode:

```json
{
  "name": "opencode",
  "supportsPersistentBackend": true,
  "transportModes": ["oneShotProcess", "persistentBackend"]
}
```

Programmatic callers can use:

```rust
use wrangle_runner::{available_backends, find_backend};

let backends = available_backends();
for b in &backends {
    println!("{}: persistent={}", b.name, b.supports_persistent_backend);
}

let opencode = find_backend("opencode").unwrap();
assert!(opencode.supports_persistent_backend);
```

## Trust model

### One-shot trust

- Each execution gets a fresh process with no prior state
- File system changes from previous executions are visible but no in-process state leaks
- Permission scope is per-invocation

### Persistent backend trust

- The persistent server process lives across multiple invocations
- In-process state (conversation history, loaded models, tool state) persists between calls
- Permission scope is per-session, not per-invocation
- The server process runs under the same user as wrangle

### Trust implications

1. A persistent server crash may lose in-flight work that was not persisted
2. Session state from one caller may be visible to another caller using the same server
3. Permission decisions made in one session may carry forward within the server's lifetime
4. A misbehaving caller could leave the server in a bad state for subsequent callers

Downstream orchestration systems should:

- Use separate sessions for unrelated work
- Not assume the server process is always clean
- Handle `PersistentBackend` fallback to `OneShotProcess` gracefully
- Clean up sessions when done using the close/resume semantics

## Fallback behavior

When `TransportMode::PersistentBackend` is requested:

1. wrangle checks if the backend supports persistent mode
2. If not supported, falls back to `OneShotProcess` immediately
3. If supported, attempts persistent execution
4. If persistent execution fails, falls back to `OneShotProcess` without the session

Fallback is transparent to the caller — the `ExecutionResult` will report the
actual transport mode used (`transport` field), so callers can detect when
fallback occurred.

## Usage

### CLI

```bash
wrangle --transport persistent-backend "fix the bug"
```

### Library

```rust
use wrangle_runner::{Runner, RuntimeConfig, TransportMode, ExecutionRequest};
use std::path::PathBuf;

let config = RuntimeConfig {
    backend: Some("opencode".to_string()),
    transport_mode: TransportMode::PersistentBackend,
    ..RuntimeConfig::default()
};

let runner = Runner::new(config);
let result = runner.execute_task("fix the bug").await?;

// Check what transport was actually used
println!("transport: {:?}", result.transport);
println!("success: {}", result.success);
```

## Session states

| State | Transport | Meaning |
|---|---|---|
| `Ephemeral` | any | No session tracking |
| `Resumable` | `OneShotProcess` | Session can be resumed with `-r` |
| `PersistentAttached` | `PersistentBackend` | Session is attached to persistent server |
