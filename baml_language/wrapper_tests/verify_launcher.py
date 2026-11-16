#!/usr/bin/env python3
"""Verify generated launchers against real packed wrappers, recording only locally."""

import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest

from environment import test_environment


ROOT = Path(__file__).resolve().parents[2]
MARKER = "BAML_WRAPPER_LAUNCHER_DEFAULT"
PROCESS_ENV_KEYS = {
    "PATH",
    "SYSTEMROOT",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
    "TEMP",
    "TMP",
    "TMPDIR",
    "LD_LIBRARY_PATH",
    "DYLD_LIBRARY_PATH",
    "LANG",
    "LC_ALL",
    "__CF_USER_TEXT_ENCODING",
    "PIP_CERT",
}


class PackedLauncherTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="packed-launcher-")
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.root = Path(cls.temporary.name)
        cls.environment = dict(
            test_environment(
                {
                    key: value
                    for key, value in os.environ.items()
                    if key in PROCESS_ENV_KEYS
                }
            ),
            BAML_CLI_ALLOW_DIRECT="1",
            DEV_BAML_CLI_DISABLE_AGENT_DETECTION="1",
        )
        # No ambient credentials can reach probes or failed assertion output.
        cls.environment.update(HOME=str(cls.root), USERPROFILE=str(cls.root))
        cls.environment["BAML_HOME"] = str(cls.root / "build-home")
        cls.launchers = {}
        cls.binaries = {}
        suffix = ".exe" if os.name == "nt" else ""
        for default in ("high", "low"):
            binary = cls.root / f"baml-{default}{suffix}"
            launcher = cls.root / f"baml-{default}.py"
            subprocess.run(
                [
                    sys.executable,
                    ROOT / "baml_language/wrapper/build",
                    "--cli",
                    CLI,
                    "--pack-host",
                    HOST,
                    "--no-self-update",
                    "--output",
                    binary,
                    "--launcher",
                    launcher,
                    "--launcher-telemetry-default",
                    default,
                ],
                env=cls.environment,
                check=True,
                capture_output=True,
                text=True,
                timeout=180,
            )
            cls.launchers[default] = launcher
            cls.binaries[default] = binary

    def case(self, name, level=None):
        root = self.root / name
        root.mkdir()
        (root / "baml.toml").write_text('[package]\nname = "launcher-test"\n')
        environment = dict(self.environment, BAML_HOME=str(root / ".baml"))
        if level is None:
            environment.pop("BAML_TELEMETRY", None)
        else:
            environment["BAML_TELEMETRY"] = level
        return root, environment

    def run_wrapper(self, default, root, environment, args, *, direct=False):
        command = (
            [self.binaries[default]]
            if direct
            else [sys.executable, self.launchers[default]]
        )
        return subprocess.run(
            [*command, *args],
            cwd=root,
            env=environment,
            input="",
            capture_output=True,
            text=True,
            timeout=30,
        )

    def query(self, root, environment, sql):
        # Disable recording for the reader itself, and force the local backend.
        result = subprocess.run(
            [
                CLI,
                "--output-format",
                "json",
                "local",
                "recordings",
                "query",
                sql,
            ],
            cwd=root,
            env=dict(environment, BAML_TELEMETRY="off"),
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        return report["rows"]

    def test_startup_observation_matches_selected_and_explicit_levels(self):
        for default in ("high", "low"):
            for index, (value, expected) in enumerate(
                [
                    (None, default),
                    ("high", "high"),
                    ("low", "low"),
                    ("medium", "medium"),
                    ("off", "off"),
                    ("", "medium"),
                    (" HIGH ", "high"),
                    ("false", "off"),
                    ("true", "medium"),
                    ("invalid", "off"),
                ]
            ):
                with self.subTest(default=default, value=value):
                    root, environment = self.case(f"startup-{default}-{index}", value)
                    result = self.run_wrapper(default, root, environment, ["--help"])
                    self.assertEqual(result.returncode, 0, result.stderr)
                    recordings = list((root / ".baml/btel/recordings").rglob("*.btel"))
                    if expected == "off":
                        self.assertEqual(recordings, [])
                        continue
                    self.assertTrue(recordings, f"no local recording for {expected}")
                    rows = self.query(
                        root,
                        environment,
                        "SELECT span_name FROM span_announcements WHERE span_type = 'function'",
                    )
                    names = [row[0] for row in rows]
                    # main is automatic and starts before any BAML statements.
                    self.assertEqual(
                        any(name.endswith(".main") for name in names),
                        expected == "high",
                        names,
                    )
                    self.assertTrue(any("dispatch" in name for name in names), names)
                    timings = self.query(
                        root,
                        environment,
                        "SELECT function_name FROM profiler WHERE invocation_count > 0",
                    )
                    self.assertEqual(
                        any(row[0] and row[0].endswith(".main") for row in timings),
                        expected in ("medium", "high"),
                        timings,
                    )

    def test_delegation_preserves_explicit_level_and_removes_only_launcher_default(
        self,
    ):
        probe = self.root / "delegated.py"
        probe.write_text(
            "import json, os, sys\n"
            "print(json.dumps(dict(args=sys.argv[1:], cwd=os.getcwd(), env=dict(os.environ))))\n"
            "print('delegated stderr', file=sys.stderr)\n"
            "sys.exit(37)\n"
        )
        for default in ("high", "low"):
            for index, value in enumerate((None, "off", "low", "high", "medium", "")):
                with self.subTest(default=default, value=value):
                    root, environment = self.case(
                        f"delegation-{default}-{index}", value
                    )
                    environment.update(
                        BAML_TOOLCHAIN=sys.executable, UNRELATED="preserved"
                    )
                    args = ["", "two words", 'a"b', "private-argument-sentinel"]
                    result = self.run_wrapper(
                        default, root, environment, [str(probe), *args]
                    )
                    self.assertEqual(result.returncode, 37, result.stderr)
                    payload = json.loads(result.stdout)
                    self.assertEqual(payload["args"], args)
                    self.assertTrue(os.path.samefile(payload["cwd"], root))
                    self.assertNotIn(MARKER, payload["env"])
                    expected = dict(environment, BAML_CLI_ALLOW_DIRECT="1")
                    self.assertEqual(payload["env"], expected)
                    self.assertEqual(result.stderr, "delegated stderr\n")
                    if value != "off":
                        rows = self.query(
                            root,
                            environment,
                            "SELECT input_args, output_value, error_value FROM spans",
                        )
                        self.assertNotIn("private-argument-sentinel", json.dumps(rows))
                        # #5621 may retain its sanitized child exit-code output;
                        # raw arguments and error payloads must remain absent.
                        self.assertTrue(
                            all(row[0] is None and row[2] is None for row in rows), rows
                        )

    def test_default_does_not_enable_recording_in_detached_flush_helper(self):
        captured = threading.Event()

        class Capture(BaseHTTPRequestHandler):
            def do_POST(self):
                self.rfile.read(int(self.headers["Content-Length"]))
                self.send_response(200)
                self.end_headers()
                captured.set()

            def log_message(self, *args):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Capture)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            for default in ("high", "low"):
                with self.subTest(default=default):
                    captured.clear()
                    root, environment = self.case(f"detached-{default}")
                    environment.update(
                        DO_NOT_TRACK="0",
                        BAML_WRAPPER_TELEMETRY_HOST=f"http://127.0.0.1:{server.server_port}",
                    )
                    result = self.run_wrapper(default, root, environment, ["--help"])
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertTrue(
                        captured.wait(15),
                        "detached helper did not reach local capture server",
                    )
                    rows = self.query(
                        root, environment, "SELECT process_id FROM processes"
                    )
                    self.assertEqual(
                        len(rows), 1, "detached helper unexpectedly created a recording"
                    )
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)

    def test_direct_binary_keeps_existing_default_and_usage_errors_keep_status(self):
        root, environment = self.case("direct")
        result = self.run_wrapper("high", root, environment, ["--help"], direct=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        rows = self.query(
            root,
            environment,
            "SELECT span_name FROM span_announcements WHERE span_type = 'function'",
        )
        self.assertFalse(any(row[0].endswith(".main") for row in rows), rows)
        for default in ("high", "low"):
            root, environment = self.case(f"error-{default}", "off")
            direct = self.run_wrapper(
                default, root, environment, ["toolchain", "use"], direct=True
            )
            launched = self.run_wrapper(
                default, root, environment, ["toolchain", "use"]
            )
            self.assertNotEqual(direct.returncode, 0)
            self.assertEqual(launched.returncode, direct.returncode)
            self.assertEqual(launched.stdout, direct.stdout)
            self.assertEqual(launched.stderr, direct.stderr)

    def test_recorded_errors_exclude_raw_payloads_at_high_default(self):
        root, environment = self.case("private-error")
        sentinel = "private-error-sentinel/invalid-selector"
        result = self.run_wrapper(
            "high", root, environment, ["toolchain", "use", sentinel]
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(sentinel, result.stderr)
        rows = self.query(
            root, environment, "SELECT input_args, output_value, error_value FROM spans"
        )
        self.assertTrue(rows)
        self.assertTrue(all(row[0] is None and row[2] is None for row in rows), rows)
        self.assertNotIn(sentinel, json.dumps(rows))

    def test_helpers_invoked_through_launcher_do_not_apply_selected_default(self):
        for default in ("high", "low"):
            root, environment = self.case(f"direct-helper-{default}")
            result = self.run_wrapper(
                default, root, environment, ["__flush-wrapper-telemetry"]
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(list((root / ".baml/btel/recordings").rglob("*.btel")), [])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--pack-host", required=True, type=Path)
    args, remaining = parser.parse_known_args()
    CLI, HOST = str(args.cli.resolve()), str(args.pack_host.resolve())
    unittest.main(argv=[sys.argv[0], *remaining])
