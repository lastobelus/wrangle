# codeagent-wrapper (Rust)

[English](README.md) | [简体中文](README.zh-CN.md)

A high-performance wrapper for AI CLI backends (Claude, Codex, Gemini, Opencode).

## Features

- 🚀 **Blazing fast**: ~6ms cold start (vs ~80ms Node.js) - **13x faster**
- 📦 **Zero dependencies**: Single binary (~2.1MB), no runtime needed
- 💾 **Low memory**: ~3MB (vs ~35MB Node.js) - **12x less**
- 🔌 **Multi-backend**: Claude, Codex, Gemini, Opencode
- ⚡ **Parallel execution**: DAG-based task orchestration
- 🔄 **Session resume**: Continue previous conversations
- 🌍 **Cross-platform**: macOS, Linux, Windows

## Installation

### Pre-built binaries (Recommended)

Download the latest release for your platform:

```bash
# macOS (Apple Silicon)
curl -L https://github.com/localSummer/codeagent-wrapper-node/releases/latest/download/codeagent-aarch64-apple-darwin -o codeagent-wrapper
chmod +x codeagent-wrapper
sudo mv codeagent-wrapper /usr/local/bin/

# macOS (Intel)
curl -L https://github.com/localSummer/codeagent-wrapper-node/releases/latest/download/codeagent-x86_64-apple-darwin -o codeagent-wrapper
chmod +x codeagent-wrapper
sudo mv codeagent-wrapper /usr/local/bin/

# Linux (x86_64)
curl -L https://github.com/localSummer/codeagent-wrapper-node/releases/latest/download/codeagent-x86_64-unknown-linux-gnu -o codeagent-wrapper
chmod +x codeagent-wrapper
sudo mv codeagent-wrapper /usr/local/bin/

# Linux (ARM64)
curl -L https://github.com/localSummer/codeagent-wrapper-node/releases/latest/download/codeagent-aarch64-unknown-linux-gnu -o codeagent-wrapper
chmod +x codeagent-wrapper
sudo mv codeagent-wrapper /usr/local/bin/
```

### Homebrew (macOS/Linux)

```bash
brew tap localSummer/codeagent
brew install codeagent-wrapper
```

### Cargo (from source)

```bash
cd codeagent-wrapper-rs
cargo install --path .
```

### From crates.io

```bash
cargo install codeagent-wrapper
```

## Usage

### Basic usage

```bash
# Run a task with auto-detected backend
codeagent-wrapper "Fix the bug in main.rs"

# Specify a backend
codeagent-wrapper --backend claude "Implement feature X"

# Specify a model
codeagent-wrapper --backend codex --model gpt-4 "Optimize this function"
```

### Agent presets

Use `-a, --agent <AGENT>` to apply a preset from `~/.codeagent/models.json`:

```bash
codeagent-wrapper --agent oracle "Analyze this codebase"
codeagent-wrapper --agent develop "Implement feature X"
```

Built-in presets (used when `~/.codeagent/models.json` is missing):

| Agent                     | Backend  | Model               |
| ------------------------- | -------- | ------------------- |
| `oracle`                  | `claude` | `claude-sonnet-4-6` |
| `librarian`               | `claude` | `claude-sonnet-4-6` |
| `explore`                 | `codex`  | (unset)             |
| `develop`                 | `codex`  | (unset)             |
| `frontend-ui-ux-engineer` | `gemini` | (unset)             |
| `document-writer`         | `gemini` | (unset)             |

### Override behavior

When `--agent` is set, runtime values are resolved in this order:

1. Explicit CLI flags (highest): `--backend`, `--model`, `--prompt-file`, `--reasoning-effort`, `--skip-permissions`
2. Agent preset values in `models.json`
3. Backend auto-detection if backend is still unset

In parallel mode, task-level fields are applied before global CLI flags, then agent preset fallback.

Examples:

```bash
# Override backend/model from CLI
codeagent-wrapper --agent oracle --backend codex --model gpt-5.2 "Implement feature X"

# Override model from CLI
codeagent-wrapper --agent oracle --model claude-opus-4-5 "Deep architecture review"

# Override prompt file from CLI
codeagent-wrapper --agent oracle --prompt-file ./prompts/review.md "Review this module"
```

### Resume a session

```bash
codeagent-wrapper resume abc123 "Continue the implementation"
```

### Parallel execution

```bash
cat tasks.txt | codeagent-wrapper --parallel
```

### Install skill

```bash
codeagent-wrapper init
```

### Cleanup old logs

```bash
codeagent-wrapper --cleanup
```

## Configuration

### Environment variables

| Variable                     | Description              |
| ---------------------------- | ------------------------ |
| `CODEAGENT_BACKEND`          | Default backend          |
| `CODEAGENT_MODEL`            | Default model            |
| `CODEX_TIMEOUT`              | Task timeout in seconds  |
| `CODEAGENT_SKIP_PERMISSIONS` | Skip permission checks   |
| `CODEAGENT_QUIET`            | Suppress progress output |
| `CODEAGENT_DEBUG`            | Enable debug logging     |

### Config files

- `~/.codeagent/models.json` - Agent and model config (`defaultBackend`, `defaultModel`, `agents`)
- Agent fields: `backend`, `model`, `promptFile`, `reasoningEffort`, `skipPermissions`
- If `~/.codeagent/models.json` does not exist, built-in defaults are used (matching the current default models.json content)

## Performance

Measured on Apple M1 Pro, macOS 14.0

| Metric                   | Node.js   | Rust          | Improvement    |
| ------------------------ | --------- | ------------- | -------------- |
| Cold start               | ~80ms     | **6ms**       | **13x faster** |
| JSON parsing (1K events) | ~23ms     | **1.03ms**    | **22x faster** |
| JSON throughput          | ~10 MiB/s | **100 MiB/s** | **10x faster** |
| Memory usage             | ~35MB     | **~3MB**      | **12x less**   |
| Binary size              | N/A       | **2.1MB**     | Single file    |

### Benchmark details

```
JSON Parsing Performance:
- parse_1000_events: 1.0260ms (974.66 Kelem/s)
- parse_10k_events:  9.7982ms (100.14 MiB/s)
```

## Migration from Node.js

### Drop-in replacement

The Rust version is a drop-in replacement for the Node.js version:

```bash
# Before (Node.js)
npx codeagent-wrapper "Your task"

# After (Rust)
codeagent-wrapper "Your task"
```

### Compatibility

- ✅ All CLI flags and options
- ✅ Config file format (models.json)
- ✅ Environment variables
- ✅ Session resume functionality
- ✅ Parallel execution
- ✅ All backends (Claude, Codex, Gemini, Opencode)
- ✅ Claude backend appends `--verbose` for `-p --output-format stream-json` compatibility (Claude Code 2.1.62+)
- ✅ Claude backend keeps Claude CLI default settings source resolution (no forced `--disable-settings-source`)

### Breaking changes

None. The Rust version maintains full API compatibility.

## Development

### Build

```bash
cargo build --release
```

### Run tests

```bash
cargo test
```

### Run benchmarks

```bash
cargo bench
```

### Format code

```bash
cargo fmt
```

### Lint

```bash
cargo clippy -- -D warnings
```

## Code Quality

This project maintains strict code quality standards:

- ✅ Zero Clippy warnings (`-D warnings` flag)
- ✅ Consistent formatting with `rustfmt`
- ✅ Comprehensive test coverage (38 unit tests + 5 integration tests)
- ✅ Reserved APIs documented with `#![allow(dead_code)]` comments

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Ensure code quality:
   ```bash
   cargo fmt
   cargo clippy -- -D warnings
   cargo test
   ```
4. Commit your changes (`git commit -m 'feat: add amazing feature'`)
5. Push to the branch (`git push origin feature/amazing-feature`)
6. Open a Pull Request

## License

MIT
