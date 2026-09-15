"""Offline compiler-cache checks. Run with python3 tools/atb2/deploy/test_entrypoint.py."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ENTRYPOINT = Path(__file__).with_name("build-cli.sh")
PIN = "a" * 40


class EntrypointTests(unittest.TestCase):
    def boot(self, *, pin=PIN, cached=PIN, executable=True, fetch_ok=False, cargo_ok=True,
             toolchain="canary", nightly=None):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            commands = root / "bin"
            commands.mkdir()
            runner = root / "data"
            (runner / "repo/.git").mkdir(parents=True)
            (runner / "repo/baml_language").mkdir()
            target = runner / "target"
            (target / "debug").mkdir(parents=True)
            (target / ".baml-cli-rev").write_text(cached)
            cli = target / "debug/baml-cli"
            cli.write_text("#!/bin/sh\nexit 0\n")
            cli.chmod(0o755 if executable else 0o644)
            log = root / "calls"
            scripts = {
                "git": (
                    f'echo "git $*" >> "{log}"\n'
                    'case "$1" in\n'
                    f"fetch) exit {0 if fetch_ok else 42} ;;\n"
                    f"rev-parse) echo {PIN} ;;\n"
                    "esac\n"
                ),
                "cargo": f'echo cargo >> "{log}"\nexit {0 if cargo_ok else 43}\n',
                "curl": (
                    f'echo "curl" >> "{log}"\n'
                    + (f"echo '{{\"version\": \"{nightly}\"}}'\n" if nightly else "exit 22\n")
                ),
            }
            for name, script in scripts.items():
                path = commands / name
                path.write_text("#!/bin/sh\n" + script)
                path.chmod(0o755)
            env = {
                "PATH": str(commands) + os.pathsep + "/usr/bin:/bin",
                "ATB2_HOME": str(runner),
                "INFISICAL_TOKEN": "offline-test-placeholder",
                "INFISICAL_PROJECT_ID": "offline-test",
            }
            if pin:
                env["ATB2_CANARY_REV"] = pin
            if toolchain:
                env["ATB2_TOOLCHAIN"] = toolchain
            result = subprocess.run(
                ["bash", str(ENTRYPOINT)], env=env, capture_output=True, text=True
            )
            calls = log.read_text() if log.exists() else ""
            marker = target / ".baml-cli-rev"
            version = target / ".baml-cli-version"
            self.version = version.read_text().strip() if version.exists() else ""
            return result, calls, marker.read_text().strip() if marker.exists() else ""

    def test_matching_pin_boots_without_network(self):
        result, calls, _ = self.boot()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, "")

    def test_mismatched_pin_requires_fetch(self):
        result, calls, _ = self.boot(cached="b" * 40)
        self.assertEqual(result.returncode, 42)
        self.assertEqual(calls, "git fetch -q origin canary\n")

    def test_nonexecutable_cache_requires_fetch(self):
        result, calls, _ = self.boot(executable=False)
        self.assertEqual(result.returncode, 42)
        self.assertEqual(calls, "git fetch -q origin canary\n")

    def test_tracking_canary_requires_fetch(self):
        result, calls, _ = self.boot(pin=None)
        self.assertEqual(result.returncode, 42)
        self.assertEqual(calls, "git fetch -q origin canary\n")

    def test_stale_pin_rebuilds_after_fetch(self):
        result, calls, revision = self.boot(cached="b" * 40, fetch_ok=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("git fetch -q origin canary\n", calls)
        self.assertIn("git checkout -q --detach " + PIN + "\n", calls)
        self.assertIn("cargo\n", calls)
        self.assertEqual(revision, PIN)

    def test_failed_rebuild_invalidates_old_revision(self):
        result, calls, revision = self.boot(cached="b" * 40, fetch_ok=True, cargo_ok=False)
        self.assertEqual(result.returncode, 43)
        self.assertIn("cargo\n", calls)
        self.assertEqual(revision, "")

    def test_latest_nightly_is_built_from_its_tag(self):
        result, calls, revision = self.boot(
            pin=None, cached="b" * 40, fetch_ok=True, toolchain=None,
            nightly="0.18.1-nightly.20260908.a",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("curl\n", calls)
        self.assertIn(
            "git fetch -q --no-tags origin refs/tags/baml-language-0.18.1-nightly.20260908.a\n", calls
        )
        self.assertNotIn("origin canary", calls)
        self.assertEqual(revision, PIN)
        self.assertEqual(self.version, "0.18.1-nightly.20260908.a")

    def test_nightly_manifest_failure_builds_nothing(self):
        result, calls, revision = self.boot(pin=None, cached="b" * 40, fetch_ok=True, toolchain=None)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("cargo\n", calls)
        self.assertEqual(revision, "b" * 40)

    def test_manifest_without_a_nightly_version_is_refused(self):
        result, calls, _ = self.boot(pin=None, cached="b" * 40, fetch_ok=True, toolchain=None, nightly="0.18.0")
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("cargo\n", calls)

    def test_explicit_pin_still_wins_over_the_nightly(self):
        result, calls, revision = self.boot(cached="b" * 40, fetch_ok=True, toolchain=None)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("curl\n", calls)
        self.assertEqual(revision, PIN)


if __name__ == "__main__":
    unittest.main()
