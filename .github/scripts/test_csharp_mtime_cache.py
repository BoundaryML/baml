#!/usr/bin/env python3
"""Regression checks for the two C# cache-restore phases; no .NET build needed."""

import contextlib
import importlib.util
import io
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "csharp_mtime_cache", Path(__file__).with_name("csharp-mtime-cache.py")
)
cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cache)


class RestoreTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        self.roots = [root / name for name in ("bridge", "fixtures", "proto")]
        overrides = patch.multiple(
            cache,
            ROOTS=[path.as_posix() for path in self.roots],
            MANIFEST=str(root / "cache" / "manifest.json"),
        )
        overrides.start()
        self.addCleanup(overrides.stop)
        # Script progress output is not part of the contract under test.
        self.output = contextlib.redirect_stdout(io.StringIO())
        self.output.__enter__()
        self.addCleanup(self.output.__exit__, None, None, None)
        self.source = self.roots[0] / "Runtime.cs"
        self.proto = self.roots[2] / "call.proto"
        self.generated = self.roots[1] / "basic" / "baml_sdk" / "Program.g.cs"
        for path in (self.source, self.proto, self.generated):
            self.write(path, "original")
        self.outputs = [
            self.roots[0] / "bin" / "Debug" / "bridge.dll",
            self.roots[1] / "basic" / "obj" / "generated.cs",
        ]
        for path in self.outputs:
            self.write(path, "cached output")
        cache.record()

    @staticmethod
    def write(path, content):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)

    def assert_outputs_exist(self, expected):
        for path in self.outputs:
            self.assertEqual(path.exists(), expected, str(path))

    def test_missing_clients_regenerated_unchanged_keep_cached_outputs(self):
        self.generated.unlink()
        cache.restore(before_codegen=True)
        self.assert_outputs_exist(True)
        self.assertEqual(self.source.stat().st_mtime, cache.BACKDATED_MTIME)
        self.write(self.generated, "original")
        cache.restore()
        self.assert_outputs_exist(True)
        self.assertEqual(self.generated.stat().st_mtime, cache.BACKDATED_MTIME)

    def test_interrupted_codegen_recovers_without_publishing_new_hashes(self):
        manifest = Path(cache.MANIFEST).read_bytes()
        self.generated.unlink()
        cache.restore(before_codegen=True)
        self.write(self.generated, "changed before interruption")
        self.assert_outputs_exist(True)
        # Failure skips the workflow's record/save steps. A later checkout
        # still compares regenerated clients against the last validated build.
        self.assertEqual(Path(cache.MANIFEST).read_bytes(), manifest)
        self.generated.unlink()
        cache.restore(before_codegen=True)
        self.write(self.generated, "changed before interruption")
        cache.restore()
        self.assert_outputs_exist(False)

    def test_missing_clients_are_invalidated_after_codegen(self):
        self.generated.unlink()
        cache.restore(before_codegen=True)
        self.assert_outputs_exist(True)
        cache.restore()
        self.assert_outputs_exist(False)

    def test_changed_regenerated_clients_invalidate_outputs(self):
        self.generated.unlink()
        cache.restore(before_codegen=True)
        self.write(self.generated, "changed")
        cache.restore()
        self.assert_outputs_exist(False)

    def test_changed_proto_invalidates_even_before_codegen(self):
        self.generated.unlink()
        self.write(self.proto, "changed proto")
        # A changed input can still be older than a restored generated output.
        os.utime(self.proto, (cache.BACKDATED_MTIME + 1,) * 2)
        cache.restore(before_codegen=True)
        self.assert_outputs_exist(False)

    def test_missing_ordinary_source_is_not_deferred(self):
        self.source.unlink()
        cache.restore(before_codegen=True)
        self.assert_outputs_exist(False)

    def test_new_generated_client_invalidates_outputs(self):
        self.write(self.generated.with_name("Added.g.cs"), "new")
        cache.restore()
        self.assert_outputs_exist(False)


if __name__ == "__main__":
    unittest.main()
