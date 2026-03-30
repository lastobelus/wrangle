#!/usr/bin/env python3
"""Resolve wrangle offload requests and execute wrangle in quiet final-result mode."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any

SUPPORTED_BACKENDS = {"codex", "claude", "gemini", "opencode", "qwen"}
DEFAULT_ENV_KEYS = [
    "PATH",
    "HOME",
    "USER",
    "SHELL",
    "TERM",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TMPDIR",
    "TMP",
    "TEMP",
]


class UsageError(ValueError):
    pass


@dataclass
class ResolvedRequest:
    backend: str
    task: str
    model: str | None
    cwd: Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Resolve natural-language wrangle offload requests."
    )
    parser.add_argument("--utterance", help="Original user request to parse.")
    parser.add_argument("--backend", help="Explicit backend override.")
    parser.add_argument("--model", help="Explicit model override.")
    parser.add_argument("--task", help="Explicit task override.")
    parser.add_argument("--cwd", default=".", help="Working directory for wrangle.")
    parser.add_argument("--last-backend", help="Fallback backend from recent context.")
    parser.add_argument(
        "--config",
        default=".codex/wrangle.toml",
        help="Path to wrangle plugin config TOML.",
    )
    parser.add_argument(
        "--dry-run", action="store_true", help="Show resolved command without running it."
    )
    return parser.parse_args()


def load_config(path: Path) -> dict[str, Any]:
    if not path.exists():
        raise UsageError(f"config file not found: {path}")
    with path.open("rb") as handle:
        data = tomllib.load(handle)
    command = data.get("wrangle_command")
    if not isinstance(command, list) or not all(isinstance(item, str) for item in command):
        raise UsageError("wrangle_command must be an array of strings")
    return data


def normalize_backend(value: str | None) -> str | None:
    if value is None:
        return None
    backend = value.strip().lower()
    if backend not in SUPPORTED_BACKENDS:
        raise UsageError(
            f"unsupported backend '{value}'. Supported backends: {', '.join(sorted(SUPPORTED_BACKENDS))}"
        )
    return backend


def strip_wrangle_suffix(text: str) -> str:
    return re.sub(r"\s*\.?\s*use wrangle\s*$", "", text, flags=re.IGNORECASE).strip()


def parse_utterance(utterance: str, last_backend: str | None) -> tuple[str | None, str | None, str]:
    text = utterance.strip()

    patterns = [
        re.compile(
            r"^use wrangle to tell (?P<backend>[a-z0-9_-]+)(?: with (?P<model>\S+))? to (?P<task>.+)$",
            re.IGNORECASE,
        ),
        re.compile(
            r"^tell (?P<backend>[a-z0-9_-]+)(?: with (?P<model>\S+))? to (?P<task>.+)$",
            re.IGNORECASE,
        ),
    ]

    for pattern in patterns:
        match = pattern.match(text)
        if match:
            return (
                normalize_backend(match.group("backend")),
                match.group("model"),
                strip_wrangle_suffix(match.group("task")),
            )

    if re.search(r"\buse wrangle\s*$", text, re.IGNORECASE):
        task = strip_wrangle_suffix(text)
        return normalize_backend(last_backend), None, task

    raise UsageError(
        "could not parse request. Use forms like 'use wrangle to tell opencode to ...' or 'tell claude with claude-sonnet-4-6 to ...'."
    )


def resolve_request(args: argparse.Namespace, config: dict[str, Any]) -> ResolvedRequest:
    backend = normalize_backend(args.backend)
    task = args.task.strip() if args.task else None
    model = args.model

    if args.utterance and (backend is None or task is None):
        parsed_backend, parsed_model, parsed_task = parse_utterance(
            args.utterance, args.last_backend
        )
        backend = backend or parsed_backend
        model = model or parsed_model
        task = task or parsed_task

    if backend is None:
        raise UsageError("backend is required. Pass --backend or provide an utterance with an explicit backend or a reliable --last-backend.")
    if not task:
        raise UsageError("task is required. Pass --task or provide a parseable utterance.")

    backend_defaults = config.get("backend_defaults", {}).get(backend, {})
    if model is None:
        default_model = backend_defaults.get("model")
        if isinstance(default_model, str) and default_model.strip():
            model = default_model.strip()

    cwd = Path(args.cwd).expanduser().resolve()
    return ResolvedRequest(backend=backend, task=task, model=model, cwd=cwd)


def build_env(config: dict[str, Any]) -> dict[str, str]:
    inherit_env = bool(config.get("inherit_env", False))
    if inherit_env:
        env = dict(os.environ)
    else:
        env = {key: os.environ[key] for key in DEFAULT_ENV_KEYS if key in os.environ}

    for key in config.get("pass_through_env", []):
        if isinstance(key, str) and key in os.environ:
            env[key] = os.environ[key]

    for key, value in config.get("env", {}).items():
        env[str(key)] = str(value)

    return env


def make_progress_file() -> Path:
    handle = tempfile.NamedTemporaryFile(
        prefix="wrangle-progress-", suffix=".jsonl", delete=False
    )
    handle.close()
    return Path(handle.name)


def tail_file(path: Path, limit: int = 10) -> list[str]:
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError:
        return []
    return lines[-limit:]


def build_wrangle_argv(
    config: dict[str, Any], request: ResolvedRequest, progress_file: Path
) -> list[str]:
    timeout_secs = int(config.get("timeout_secs", 7200))
    permission_policy = str(config.get("default_permission_policy", "default"))
    argv = list(config["wrangle_command"])
    argv.extend(["--backend", request.backend])
    if request.model:
        argv.extend(["--model", request.model])
    argv.extend(
        [
            "--timeout",
            str(timeout_secs),
            "--permission-policy",
            permission_policy,
            "--progress-file",
            str(progress_file),
            "--quiet-until-complete",
        ]
    )
    if bool(config.get("inherit_env", False)):
        argv.append("--inherit-env")
    argv.extend([request.task, str(request.cwd)])
    return argv


def recommended_yield_time_ms(timeout_secs: int) -> int:
    return (timeout_secs + 30) * 1000


def run_wrangle(
    argv: list[str], cwd: Path, env: dict[str, str], progress_file: Path, timeout_secs: int
) -> tuple[int, dict[str, Any]]:
    completed = subprocess.run(
        argv,
        cwd=str(cwd),
        env=env,
        capture_output=True,
        text=True,
        timeout=timeout_secs + 30,
        check=False,
    )

    stdout = completed.stdout.strip()
    parsed_stdout: Any
    if stdout:
        try:
            parsed_stdout = json.loads(stdout)
        except json.JSONDecodeError:
            parsed_stdout = stdout
    else:
        parsed_stdout = None

    summary = {
        "success": completed.returncode == 0,
        "exitCode": completed.returncode,
        "stdout": parsed_stdout,
        "stderrExcerpt": completed.stderr.strip() or None,
        "progressFile": str(progress_file),
        "progressTail": tail_file(progress_file) if completed.returncode != 0 else None,
    }
    return completed.returncode, summary


def main() -> int:
    args = parse_args()
    config_path = Path(args.config).expanduser()
    config = load_config(config_path)
    request = resolve_request(args, config)
    progress_file = make_progress_file()
    argv = build_wrangle_argv(config, request, progress_file)
    timeout_secs = int(config.get("timeout_secs", 7200))
    env = build_env(config)

    if args.dry_run:
        print(
            json.dumps(
                {
                    "backend": request.backend,
                    "model": request.model,
                    "cwd": str(request.cwd),
                    "timeoutSecs": timeout_secs,
                    "recommendedYieldTimeMs": recommended_yield_time_ms(timeout_secs),
                    "progressFile": str(progress_file),
                    "wrangleCommand": argv,
                    "envKeys": sorted(env.keys()),
                },
                indent=2,
            )
        )
        return 0

    exit_code, result = run_wrangle(argv, request.cwd, env, progress_file, timeout_secs)
    print(
        json.dumps(
            {
                "backend": request.backend,
                "model": request.model,
                "cwd": str(request.cwd),
                "timeoutSecs": timeout_secs,
                "recommendedYieldTimeMs": recommended_yield_time_ms(timeout_secs),
                "wrangleCommand": argv,
                **result,
            },
            indent=2,
        )
    )
    return exit_code


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except UsageError as err:
        print(json.dumps({"success": False, "error": str(err)}, indent=2), file=sys.stderr)
        raise SystemExit(2)
