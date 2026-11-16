"""Isolated launcher/build contracts; no BAML runtime or cloud credentials."""

from __future__ import annotations

import contextlib
import io
import json
import os
from pathlib import Path
import runpy
import shutil
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
BUILDER = ROOT / "baml_language/wrapper/build"
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
PROBE = """
import json, os, signal, sys
if sys.argv[1:2] == ['signal']:
    os.kill(os.getpid(), signal.SIGTERM)
if sys.argv[1:2] == ['windows-status']:
    import ctypes
    ctypes.windll.kernel32.ExitProcess(ctypes.c_uint32(0xE0000001))
if sys.argv[1:2] == ['windows-break']:
    import time
    signal.signal(signal.SIGBREAK, lambda *_: sys.exit(23))
    print('ready', flush=True)
    time.sleep(10)
    sys.exit(99)
print(json.dumps(dict(args=sys.argv[1:], cwd=os.getcwd(), pid=os.getpid(), env=dict(os.environ))))
print(sys.stdin.read(), file=sys.stderr, end='')
sys.exit(int(os.environ.get('PROBE_EXIT', '0')))
"""


class WrapperLauncherTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="launcher space '")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.binary = self.root / ("packed.exe" if os.name == "nt" else "packed")
        self.launcher = self.root / "launch.py"
        self.probe = self.root / "probe.py"
        self.probe.write_text(PROBE)
        # Probe output and failed environment assertions must never contain
        # ambient credentials (including unrelated CI provider/cache secrets).
        self.environment = {
            key: value for key, value in os.environ.items() if key in PROCESS_ENV_KEYS
        }
        self.environment.update(HOME=str(self.root), USERPROFILE=str(self.root))
        # Keep the test host Python isolated from locale coercion and, on
        # Windows, able to find its installation after copying python.exe.
        self.environment.update(PYTHONCOERCECLOCALE="0", PYTHONHOME=sys.base_prefix)
        self.packs = []

    def fake_pack(self, command, **kwargs):
        # Also support the build-only compiler version probe used by #5635.
        if command[-1] == "--version":
            return subprocess.CompletedProcess(command, 0, stdout="baml-cli 1.2.3\n")
        project = Path(command[command.index("--project-dir") + 1])
        self.packs.append((command, (project / "baml.toml").read_text(), kwargs))
        if os.name == "nt":
            shutil.copy2(sys.executable, self.binary)
            for dll in Path(sys.executable).parent.glob("python*.dll"):
                shutil.copy2(dll, self.root / dll.name)
        else:
            self.binary.symlink_to(sys.executable)
        return subprocess.CompletedProcess(command, 0)

    def build(self, default="high", extra=(), *, fake_pack=None):
        command = [str(BUILDER), "--cli", sys.executable, "--output", str(self.binary)]
        if default is not None:
            command += [
                "--launcher",
                str(self.launcher),
                "--launcher-telemetry-default",
                default,
            ]
        with (
            patch.object(sys, "argv", [*command, *extra]),
            patch("subprocess.run", side_effect=fake_pack or self.fake_pack),
        ):
            runpy.run_path(str(BUILDER), run_name="__main__")

    def invoke(self, args=(), *, environment=None):
        return subprocess.run(
            [sys.executable, self.launcher, self.probe, *args],
            cwd=self.root,
            env=environment or self.environment,
            input="stdin is unchanged\n",
            capture_output=True,
            text=True,
            timeout=20,
        )

    def test_unset_defaults_before_host_starts_and_preserves_environment_and_stdio(
        self,
    ):
        for default in ("high", "low"):
            with self.subTest(default=default):
                if self.binary.is_symlink():
                    self.binary.unlink()
                self.build(default)
                self.environment.update(
                    BOUNDARY_API_KEY="local",
                    BOUNDARY_PROJECT="caller/project",
                    BOUNDARY_API_URL="http://127.0.0.1:1",
                    UNRELATED="unchanged",
                )
                args = [
                    "",
                    "space value",
                    'quote"value',
                    "$(literal)",
                    "日本語",
                    "--flag",
                ]
                result = self.invoke(args)
                self.assertEqual(result.returncode, 0, result.stderr)
                payload = json.loads(result.stdout)
                self.assertEqual(payload["args"], args)
                self.assertTrue(os.path.samefile(payload["cwd"], self.root))
                expected = dict(
                    self.environment, BAML_TELEMETRY=default, **{MARKER: default}
                )
                self.assertEqual(payload["env"], expected)
                self.assertEqual(result.stderr, "stdin is unchanged\n")

    def test_explicit_values_are_verbatim_including_empty_boolean_case_and_invalid(
        self,
    ):
        self.build("low")
        for value in (
            "high",
            "low",
            "medium",
            "off",
            "",
            " HIGH ",
            "OFF",
            "true",
            "1",
            "yes",
            "on",
            "false",
            "0",
            "no",
            "auto",
            "  ",
        ):
            with self.subTest(value=value):
                environment = dict(
                    self.environment, BAML_TELEMETRY=value, **{MARKER: "high"}
                )
                result = self.invoke(environment=environment)
                self.assertEqual(result.returncode, 0, result.stderr)
                expected = dict(environment)
                expected.pop(MARKER)
                self.assertEqual(json.loads(result.stdout)["env"], expected)

    def test_nonzero_exit_is_preserved(self):
        self.build()
        result = self.invoke(environment=dict(self.environment, PROBE_EXIT="37"))
        self.assertEqual(result.returncode, 37)

    @unittest.skipUnless(os.name == "nt", "Windows DWORD exit status contract")
    def test_windows_preserves_entire_exit_status(self):
        self.build()
        self.assertEqual(self.invoke(["windows-status"]).returncode, 0xE0000001)

    @unittest.skipUnless(os.name == "nt", "Windows shared-console Ctrl-Break contract")
    def test_windows_child_receives_console_break_and_parent_waits(self):
        self.build()
        with subprocess.Popen(
            [sys.executable, self.launcher, self.probe, "windows-break"],
            cwd=self.root,
            env=self.environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            text=True,
            creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
        ) as child:
            self.assertEqual(child.stdout.readline().strip(), "ready")
            child.send_signal(signal.CTRL_BREAK_EVENT)
            child.communicate(timeout=15)
            self.assertEqual(child.returncode, 23)

    @unittest.skipIf(os.name == "nt", "POSIX exec and signal contract")
    def test_posix_exec_preserves_pid_and_signal_exit(self):
        self.build()
        with subprocess.Popen(
            [sys.executable, self.launcher, self.probe],
            cwd=self.root,
            env=self.environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            text=True,
        ) as child:
            output, _ = child.communicate(timeout=20)
            self.assertEqual(json.loads(output)["pid"], child.pid)
            self.assertEqual(child.returncode, 0)
        self.assertEqual(self.invoke(["signal"]).returncode, -signal.SIGTERM)

    @unittest.skipIf(os.name == "nt", "POSIX native signal defaults")
    def test_posix_does_not_leak_python_sigpipe_ignore_into_native_host(self):
        self.build()
        self.binary.unlink()
        self.binary.symlink_to(shutil.which("sh"))
        result = subprocess.run(
            [sys.executable, self.launcher, "-c", "kill -PIPE $$; exit 99"],
            env=self.environment,
            capture_output=True,
            timeout=20,
        )
        self.assertEqual(result.returncode, -signal.SIGPIPE)

    def test_launch_errors_are_nonzero_and_do_not_echo_environment_or_arguments(self):
        self.build()
        self.binary.unlink()
        result = self.invoke(
            ["secret argument"],
            environment=dict(self.environment, BOUNDARY_API_KEY="secret key"),
        )
        self.assertEqual(result.returncode, 127)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "baml launcher: cannot start packed wrapper\n")

    @unittest.skipIf(os.name == "nt", "POSIX executable permission contract")
    def test_non_executable_host_returns_126(self):
        self.build()
        self.binary.unlink()
        self.binary.write_text("not executable")
        self.binary.chmod(0o644)
        self.assertEqual(self.invoke().returncode, 126)

    def test_ordinary_build_has_no_launcher_or_telemetry_policy_change(self):
        self.build(default=None)
        self.assertFalse(self.launcher.exists())
        command, manifest, kwargs = self.packs[0]
        self.assertNotIn("--embed-telemetry", command)
        self.assertIn('BAML_TELEMETRY = "BAML_TELEMETRY"', manifest)
        self.assertIn("http_bodies = false", manifest)
        self.assertEqual(kwargs["env"]["BAML_CLI_ALLOW_DIRECT"], "1")

    def test_launcher_composes_with_existing_build_and_destination_options(self):
        self.build(
            extra=[
                "--target",
                "x86_64-pc-windows-msvc",
                "--pack-host",
                "test-host.exe",
                "--version",
                "9.8.7",
                "--no-self-update",
                "--embed-telemetry",
                "--telemetry-project",
                "caller/project",
                "--telemetry-environment",
                "Test",
            ]
        )
        command, manifest, _ = self.packs[0]
        self.assertIn("--embed-telemetry", command)
        self.assertEqual(
            command[command.index("--target") + 1], "x86_64-pc-windows-msvc"
        )
        self.assertEqual(
            Path(command[command.index("--host-binary") + 1]).name, "test-host.exe"
        )
        self.assertIn('project = "caller/project"', manifest)
        self.assertIn('embed_telemetry_env = "Test"', manifest)
        self.assertIn('BOUNDARY_API_KEY = "BOUNDARY_API_KEY"', manifest)
        self.assertIn('BOUNDARY_PROJECT = "BOUNDARY_PROJECT"', manifest)
        self.assertIn("http_bodies = false", manifest)

    def test_invalid_options_fail_before_packing(self):
        cases = [
            ["--launcher", str(self.launcher)],
            ["--launcher-telemetry-default", "high"],
            ["--launcher", str(self.binary), "--launcher-telemetry-default", "low"],
            [
                "--launcher",
                str(self.root / "nested/launch.py"),
                "--launcher-telemetry-default",
                "low",
            ],
            ["--launcher", str(self.launcher), "--launcher-telemetry-default", "off"],
        ]
        for extra in cases:
            with self.subTest(extra=extra), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit):
                    self.build(default=None, extra=extra)
        self.assertEqual(self.packs, [])

    def test_pack_failure_does_not_create_a_launcher(self):
        def fail(*args, **kwargs):
            raise subprocess.CalledProcessError(42, "test compiler")

        with self.assertRaises(subprocess.CalledProcessError):
            self.build(fake_pack=fail)
        self.assertFalse(self.launcher.exists())

    def test_launcher_is_relocatable_and_ignores_cwd_and_path_decoys(self):
        self.build()
        moved = self.root / "moved"
        moved.mkdir()
        self.launcher.rename(moved / self.launcher.name)
        self.binary.rename(moved / self.binary.name)
        for dll in self.root.glob("python*.dll"):
            dll.rename(moved / dll.name)
        # A different executable in cwd/PATH must not be chosen.
        self.binary.write_text("decoy")
        result = subprocess.run(
            [sys.executable, moved / self.launcher.name, self.probe],
            cwd=self.root,
            env=self.environment,
            input="",
            text=True,
            capture_output=True,
            timeout=20,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_binary_name_is_quoted_without_recursive_template_expansion(self):
        self.binary = self.root / (
            "packed-@DEFAULT@-'.exe" if os.name == "nt" else "packed-@DEFAULT@-'"
        )
        self.build()
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
