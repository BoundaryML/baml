"""Source-generation contracts without executing a target pack host."""

import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

from environment import test_environment


REPO = Path(__file__).resolve().parents[2]


@unittest.skipIf(os.name == "nt", "fixture compiler uses a POSIX shebang")
class BuildVersions(unittest.TestCase):
    def build(self, version=None, compiler_version="baml-cli 7.6.5-canary.test"):
        with tempfile.TemporaryDirectory(prefix="wrapper-build-versions-") as directory:
            root = Path(directory)
            cli = root / "compiler"
            cli.write_text(
                f"#!{sys.executable}\n"
                "import json, os, pathlib, sys\n"
                "root = pathlib.Path(__file__).parent\n"
                "with (root / 'calls.jsonl').open('a') as log:\n"
                "    log.write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "if sys.argv[1:] == ['--version']:\n"
                "    assert os.environ['BAML_TELEMETRY'] == 'off'\n"
                "    assert os.environ['DO_NOT_TRACK'] == '1'\n"
                f"    print({compiler_version!r})\n"
                "else:\n"
                "    source = pathlib.Path(sys.argv[sys.argv.index('--from') + 1])\n"
                "    (root / 'generated.baml').write_text((source / 'baml_src/build.baml').read_text())\n"
            )
            cli.chmod(0o755)
            # A foreign host cannot run. The helper must pass it to pack untouched.
            host = root / "foreign-host"
            host.write_bytes(b"not a native executable")
            command = [
                sys.executable,
                REPO / "baml_language/wrapper/build",
                "--cli",
                cli,
                "--pack-host",
                host,
                "--target",
                "x86_64-pc-windows-msvc",
                "--output",
                root / "wrapper.exe",
            ]
            if version:
                command.extend(["--version", version])
            result = subprocess.run(
                [str(arg) for arg in command],
                env=test_environment(os.environ),
                capture_output=True,
                text=True,
                timeout=30,
            )
            calls = [
                json.loads(line)
                for line in (root / "calls.jsonl").read_text().splitlines()
            ]
            generated = root / "generated.baml"
            return result, calls, generated.read_text() if generated.exists() else ""

    def test_explicit_compiler_and_wrapper_override_are_independent(self):
        result, calls, source = self.build(
            "1.2.3-test", compiler_version="baml-cli 7.6.5-canary.test+build.42"
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('const WRAPPER_APP_VERSION = "1.2.3-test";', source)
        self.assertIn('const WRAPPER_BAML_VERSION = "7.6.5-canary.test+build.42";', source)
        self.assertIn(
            "function wrapper_version() -> string {\n    WRAPPER_APP_VERSION\n}", source
        )
        self.assertEqual(calls[0], ["--version"])
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[1][0], "pack")
        self.assertIn("--host-binary", calls[1])

    def test_default_app_version_uses_existing_wrapper_manifest(self):
        expected = re.search(
            r'^version = "([^"]+)"',
            (REPO / "baml_language/crates/baml/Cargo.toml").read_text(),
            re.M,
        )[1]
        result, _, source = self.build()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f'const WRAPPER_APP_VERSION = "{expected}";', source)
        self.assertIn('const WRAPPER_BAML_VERSION = "7.6.5-canary.test";', source)

    def test_unrecognized_compiler_version_fails_before_pack(self):
        result, calls, source = self.build(compiler_version="unidentified compiler")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unrecognized build compiler version", result.stderr)
        self.assertEqual(calls, [["--version"]])
        self.assertEqual(source, "")


if __name__ == "__main__":
    unittest.main()
