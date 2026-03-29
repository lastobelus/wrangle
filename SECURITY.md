# Security Policy

## Scope

`wrangle` executes trusted local agent backends. It is not a sandbox and should not be treated as a boundary that prevents a backend from accessing the working directory, inherited environment, or other local resources granted by the host system.

## Reporting

If you find a security issue, please report it privately to the maintainer before opening a public issue.

## Current security goals

- Avoid logging raw prompt content by default
- Bound backend stdout and stderr buffering
- Gate file-based prompt ingestion from external task specs
- Keep environment inheritance narrow by default
- Preserve a clean trust model around transports and sessions
- Advertise per-backend permission policy support and reject unsupported combinations explicitly

## Non-goals

- Preventing a trusted backend binary from reading files available to the current user
- Sandboxing backend execution
- Preventing all prompt exfiltration once a user intentionally sends data to a backend

See `docs/security-model.md` for operational detail.

