"""Unit tests for the wrangle-codex wrapper parsing helpers."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "scripts"))

from run_wrangle import (  # noqa: E402
    UsageError,
    normalize_backend,
    parse_utterance,
    recommended_yield_time_ms,
    strip_wrangle_suffix,
)


class ParseUtteranceTests(unittest.TestCase):
    def test_use_wrangle_to_tell_backend(self) -> None:
        backend, model, task = parse_utterance(
            "use wrangle to tell opencode to review this crate",
            None,
        )
        self.assertEqual(backend, "opencode")
        self.assertIsNone(model)
        self.assertEqual(task, "review this crate")

    def test_tell_backend_with_model(self) -> None:
        backend, model, task = parse_utterance(
            "tell claude with claude-sonnet-4-6 to write release notes",
            None,
        )
        self.assertEqual(backend, "claude")
        self.assertEqual(model, "claude-sonnet-4-6")
        self.assertEqual(task, "write release notes")

    def test_trailing_use_wrangle_uses_last_backend(self) -> None:
        backend, model, task = parse_utterance(
            "review this file. use wrangle",
            "qwen",
        )
        self.assertEqual(backend, "qwen")
        self.assertIsNone(model)
        self.assertEqual(task, "review this file")

    def test_unsupported_backend_raises(self) -> None:
        with self.assertRaises(UsageError):
            parse_utterance("tell gpt to do something", None)

    def test_unparseable_request_raises(self) -> None:
        with self.assertRaises(UsageError):
            parse_utterance("just do something random", None)


class HelperTests(unittest.TestCase):
    def test_strip_wrangle_suffix(self) -> None:
        self.assertEqual(strip_wrangle_suffix("fix this. use wrangle"), "fix this")
        self.assertEqual(strip_wrangle_suffix("fix this. Use Wrangle"), "fix this")

    def test_normalize_backend_case(self) -> None:
        self.assertEqual(normalize_backend("Claude"), "claude")
        self.assertEqual(normalize_backend("QWEN"), "qwen")

    def test_normalize_backend_unknown_raises(self) -> None:
        with self.assertRaises(UsageError):
            normalize_backend("unknown-ai")

    def test_recommended_yield_time_includes_buffer(self) -> None:
        self.assertEqual(recommended_yield_time_ms(7200), 7_230_000)


if __name__ == "__main__":
    unittest.main()
