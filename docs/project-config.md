# Project-Local Config

`wrangle` now supports project-local configuration under a repo-owned `.wrangle/`
directory.

## Discovery order

For each config file, `wrangle` resolves paths in this order:

1. nearest project-local `.wrangle/<file>` found by walking up from the working directory
2. home-scoped `~/.wrangle/<file>`
3. built-in defaults when no file exists

Today the supported files are:

- `.wrangle/models.json`
- `.wrangle/config.json`

`config.json` currently supports:

```json
{
  "logDir": ".logs/wrangle"
}
```

Relative `logDir` values are resolved relative to the directory that contains the
active `config.json`.

## Inspect active config

Use:

```bash
wrangle config-paths --json
```

This prints:

- project-local config dir when present
- home config dir
- active `models.json`
- active `config.json`
- resolved log directory

## Why this exists

Project-local config makes `wrangle` easier to use in:

- sandboxed hosts that need explicit writable roots
- team repos that want reproducible defaults
- automation where home-scoped hidden state is undesirable
