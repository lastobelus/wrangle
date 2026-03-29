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

- `Default`: backend default approval behavior — no flag applied, backend decides
- `Ask`: explicitly request interactive approval before each action (reserved; not yet mapped to any backend flag)
- `Auto`: semi-automatic mode — the backend proceeds with safe operations and asks before destructive ones
- `Bypass`: request full automation mode where the backend supports it

Not every backend supports every policy. Capability reporting (`available_backends()`, `wrangle backends`) lists the supported policies per backend so callers can decide before execution. Unsupported combinations produce a clear `UnsupportedPermissionPolicy` error rather than silently degrading. `Ask` is not currently advertised for any backend because no backend has a distinct flag mapping for it; using `Ask` will result in a policy-not-supported error.

### Backend policy support

| Backend | Default | Ask | Auto | Bypass |
|---------|---------|-----|------|--------|
| Codex   | yes     | —   | yes  | yes    |
| Claude  | yes     | —   | —    | yes    |
| Gemini  | yes     | —   | —    | yes    |
| Qwen    | yes     | —   | —    | yes    |
| Opencode| yes     | —   | —    | —      |

`Bypass` is only advertised where there is a defensible backend-native full-auto equivalent. `Ask` is not advertised because it currently maps to no distinct backend flag. `Auto` is only advertised where the semantics are clear enough to explain and test.

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

