# Security Model

## Trust boundaries

`wrangle` has three relevant trust boundaries:

1. The caller of `wrangle`
2. The selected backend executable or transport
3. The filesystem and environment visible to the current user

`wrangle` does not sandbox the backend. If a backend is malicious or compromised, it should be assumed capable of acting with the same local access available to the current user and process environment.

## Trusted vs untrusted inputs

Trusted inputs:

- direct CLI invocation by the local user
- explicitly selected backend binaries
- explicit prompt files passed by the local user

Less-trusted or externally supplied inputs:

- newline-delimited task specs for parallel execution
- backend stdout/stderr streams
- resumed session identifiers supplied by another system

## Current controls

- prompt content is not logged verbatim by default
- stdout JSON parsing is line-size bounded
- stderr capture is truncated to a fixed maximum
- retained normalized events are capped
- task-spec prompt files are disabled unless explicitly allowed
- environment inheritance is reduced by default

## Permission policy

`wrangle` models permissions through `PermissionPolicy`, then maps that model to backend-specific flags:

- `Default`: backend default approval behavior
- `Bypass`: request broad automation mode where the backend supports it

This abstraction is intentionally small in v1 so it can stay stable across CLI and future API transports.

## Persistent transports

Persistent transports increase the security surface because state may outlive one request. That affects:

- session attachment
- cleanup behavior
- credential lifetime
- whether execution context survives across requests

For that reason, `wrangle` keeps transport identity explicit:

- `OneShotProcess`
- `PersistentBackend`
- `WrangleServer` (reserved)

V1 implements only `OneShotProcess`. Opencode persistent transport is a planned v2 addition, and a `wrangle`-native server is a planned v3 addition.

