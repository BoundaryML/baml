"""Offline boot checks. Run with python3 tools/atb2/deploy/test_entrypoint.py."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ENTRYPOINT = Path(__file__).with_name("entrypoint.sh")
PIN = "a" * 40


class EntrypointTests(unittest.TestCase):
    def boot(self, *, pin=PIN, cached=PIN, executable=True, fetch_ok=False):
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
                "cargo": f'echo cargo >> "{log}"\n',
                # Stop at the phase-two boundary, without secrets or a live run.
                "infisical": f'echo infisical >> "{log}"\n',
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
            result = subprocess.run(
                ["bash", str(ENTRYPOINT)], env=env, capture_output=True, text=True
            )
            calls = log.read_text() if log.exists() else ""
            return result, calls, (target / ".baml-cli-rev").read_text().strip()

    def test_matching_pin_boots_without_network(self):
        result, calls, _ = self.boot()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, "infisical\n")

    def test_mismatched_pin_requires_fetch(self):
        result, calls, _ = self.boot(cached="b" * 40)
        self.assertEqual(result.returncode, 42)
        self.assertNotIn("infisical", calls)

    def test_nonexecutable_cache_requires_fetch(self):
        result, calls, _ = self.boot(executable=False)
        self.assertEqual(result.returncode, 42)
        self.assertNotIn("infisical", calls)

    def test_tracking_canary_requires_fetch(self):
        result, calls, _ = self.boot(pin=None)
        self.assertEqual(result.returncode, 42)
        self.assertNotIn("infisical", calls)

    def test_stale_pin_rebuilds_after_fetch(self):
        result, calls, revision = self.boot(cached="b" * 40, fetch_ok=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("git fetch -q origin canary\n", calls)
        self.assertIn("git checkout -q --detach " + PIN + "\n", calls)
        self.assertIn("cargo\ninfisical\n", calls)
        self.assertEqual(revision, PIN)


if __name__ == "__main__":
    unittest.main()
