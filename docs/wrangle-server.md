# Wrangle Server Transport

`WrangleServer` is the first wrangle-owned long-lived transport.

## What it does

Instead of invoking a backend directly from the caller process, `wrangle`
starts or reuses a local server process and sends normalized execution requests
to it over a local TCP connection.

The server:

- owns the long-lived transport identity
- can serve multiple local client requests
- keeps session handles stable at the wrangle layer
- delegates actual execution to CLI-backed or API-backed backends

## Session semantics

When a request goes through `WrangleServer`, the caller receives a session
handle with:

- `transport: "wrangleServer"`
- `state: "serverAttached"`

Internally the server may map that stable outer session to:

- a resumable one-shot backend session
- a persistent backend session
- or no backend session at all for stateless API adapters

This keeps the caller-facing request/result/session model stable while making
the long-lived owner explicit.

## Inner transport behavior

The first version chooses the inner execution mode automatically:

- `persistentBackend` when the selected backend supports it
- otherwise `oneShotProcess`

That means `WrangleServer` is not a replacement for the other transport modes.
It is a wrangle-owned layer that can delegate to them.

## Operational notes

- The server is started through the `wrangle server ...` subcommand.
- Registry metadata lives under `~/.wrangle/server/`.
- The first version is local-only and TCP-on-localhost only.

## Trust differences

Compared with direct one-shot execution:

- the server process is long-lived
- session ownership shifts from the backend to wrangle
- credentials and environment may stay resident in the server process lifetime
- stale server state should be treated as disposable operational state

Callers should use `WrangleServer` when they want a stable wrangle-owned attach
point, not just when they want raw backend persistence.
