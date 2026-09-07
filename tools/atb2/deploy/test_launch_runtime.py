"""Offline tests of secret filtering and the privilege-drop handoff."""

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("launcher", Path(__file__).with_name("launch-runtime.py"))
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


class LauncherTests(unittest.TestCase):
    source = {"INFISICAL_TOKEN": "machine-placeholder", "INFISICAL_PROJECT_ID": "test-project"}

    def export(self, rows, returncode=0):
        return subprocess.CompletedProcess([], returncode, json.dumps(rows), "private diagnostic")

    identity = {"INFISICAL_CLIENT_ID": "identity-fixture", "INFISICAL_CLIENT_SECRET": "secret-fixture",
                "INFISICAL_PROJECT_ID": "test-project"}

    def test_universal_auth_keeps_credentials_in_root_children_only(self):
        login = subprocess.CompletedProcess([], 0, "fresh-token\n", "private diagnostic")
        rows = [{"key": "GH_TOKEN", "value": "app-fixture"},
                {"key": "INFISICAL_CLIENT_SECRET", "value": "must-not-pass"}]
        with patch.object(launcher.subprocess, "run", side_effect=[login, self.export(rows)]) as run:
            env = launcher.runtime_environment({**self.identity, "INFISICAL_TOKEN": "stale-token"})
        auth, export = run.call_args_list
        self.assertIn("--method=universal-auth", auth.args[0])
        self.assertEqual(auth.kwargs["env"]["INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET"], "secret-fixture")
        self.assertEqual(export.kwargs["env"]["INFISICAL_TOKEN"], "fresh-token")
        self.assertNotIn("INFISICAL_UNIVERSAL_AUTH_CLIENT_SECRET", export.kwargs["env"])
        self.assertNotIn("INFISICAL_TOKEN", auth.kwargs["env"])
        for call in run.call_args_list:
            for secret in ("secret-fixture", "identity-fixture", "fresh-token", "stale-token"):
                self.assertNotIn(secret, str(call.args))
        self.assertEqual(env["GH_TOKEN"], "app-fixture")
        self.assertFalse(any(key.startswith("INFISICAL_") for key in env))

    def test_incomplete_identity_refuses_fallback(self):
        for missing in ("INFISICAL_CLIENT_ID", "INFISICAL_CLIENT_SECRET"):
            source = {key: value for key, value in self.identity.items() if key != missing}
            with patch.object(launcher.subprocess, "run") as run:
                with self.assertRaises(ValueError):
                    launcher.runtime_environment({**source, "INFISICAL_TOKEN": "old-token"})
                run.assert_not_called()

    def test_failed_or_malformed_login_never_exports(self):
        for code, output in ((1, "private output"), (0, ""), (0, "banner\ntoken"), (0, "bad\0token")):
            with self.subTest(code=code, output=output), patch.object(launcher.subprocess, "run",
                    return_value=subprocess.CompletedProcess([], code, output, "private diagnostic")) as run:
                with self.assertRaisesRegex(ValueError, "^Infisical login failed$"):
                    launcher.runtime_environment(self.identity)
                self.assertEqual(run.call_count, 1)

    def test_slack_intake_and_shepherd_config_survive_filtering(self):
        source = {"ATB_SLACK_SIGNING_SECRET": "fixture", "ATB2_SHEPHERDS": "owner:U1"}
        env = launcher.runtime_environment(source)
        self.assertEqual(env["ATB_SLACK_SIGNING_SECRET"], "fixture")
        self.assertEqual(env["ATB2_SHEPHERDS"], "owner:U1")
        self.assertNotIn("INFISICAL_TOKEN", env)

    def test_only_named_values_cross_boundary(self):
        value = "quotes'\" newline\n$(never-run); literal"
        rows = [{"key": key, "value": val} for key, val in {
            "FEEDBACK_SUPABASE_KEY": value, "INFISICAL_TOKEN": "exported-machine-token",
            "HOME": "/wrong", "LD_PRELOAD": "/wrong", "ANTHROPIC_API_KEY": "excluded",
        }.items()]
        with patch.object(launcher.subprocess, "run", return_value=self.export(rows)) as run:
            env = launcher.runtime_environment({**self.source, "ATB2_POLL_S": "60", "UNRELATED_SECRET": "excluded"})
        self.assertEqual(env["FEEDBACK_SUPABASE_KEY"], value)
        self.assertEqual(env["ATB2_POLL_S"], "60")
        self.assertEqual(env["HOME"], "/data/home")
        for key in ("INFISICAL_TOKEN", "LD_PRELOAD", "ANTHROPIC_API_KEY", "UNRELATED_SECRET"):
            self.assertNotIn(key, env)
        args, kwargs = run.call_args
        self.assertNotIn("machine-placeholder", str(args))
        self.assertIn("--expand=false", args[0])
        self.assertEqual(kwargs["env"]["INFISICAL_TOKEN"], "machine-placeholder")
        self.assertNotIn("FEEDBACK_SUPABASE_KEY", kwargs["env"])
        self.assertNotIn("shell", kwargs)

    def test_tokenless_start_still_filters_inherited_environment(self):
        with patch.object(launcher.subprocess, "run") as run:
            env = launcher.runtime_environment({"GH_TOKEN": "app-placeholder", "OTHER_SECRET": "excluded"})
        run.assert_not_called()
        self.assertEqual(env["GH_TOKEN"], "app-placeholder")
        self.assertNotIn("OTHER_SECRET", env)

    def test_failed_export_does_not_launch_or_echo_diagnostics(self):
        with patch.object(launcher.subprocess, "run", return_value=self.export([], 1)):
            with self.assertRaisesRegex(ValueError, "^Infisical export failed$"):
                launcher.runtime_environment(self.source)

    def test_invalid_export_fails_closed(self):
        for rows in ({}, [None], [{"key": "FEEDBACK_SUPABASE_KEY", "value": None}],
                     [{"key": "FEEDBACK_SUPABASE_KEY", "value": "bad\0value"}]):
            with self.subTest(rows=rows), patch.object(launcher.subprocess, "run", return_value=self.export(rows)):
                with self.assertRaises(ValueError):
                    launcher.runtime_environment(self.source)

    def test_exec_replaces_environment_before_privilege_drop(self):
        with patch.object(launcher.os, "geteuid", return_value=0), \
             patch.object(launcher.resource, "setrlimit"), \
             patch.object(launcher.os, "environ", {"INFISICAL_TOKEN": ""}), \
             patch.object(launcher.sys, "argv", ["launcher", 'request_merge("a b")']), \
             patch.object(launcher.os, "execve") as execute:
            launcher.main()
        program, argv, env = execute.call_args.args
        self.assertEqual(program, "/usr/bin/setpriv")
        self.assertEqual(argv[-1], 'request_merge("a b")')
        self.assertIn("--reuid=1000", argv)
        self.assertIn("--no-new-privs", argv)
        self.assertNotIn("INFISICAL_TOKEN", env)


if __name__ == "__main__":
    unittest.main()
