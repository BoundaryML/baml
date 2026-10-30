"""Execute installer bootstrap functions against local wrapper CLI generations."""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SELECTOR = "0.11.0"
# Include both flags in current help to check that --global takes precedence.
CASES = (
    ("current", "--global --local", "--global", 0, 0),
    ("transitional", "--local", "--local", 0, 0),
    ("legacy", "Usage: toolchain use VERSION", None, 0, 0),
    ("install failure", "--global", "--global", 0, 17),
    ("help failure", "--global", None, 19, 0),
)


def installer_function(filename: str, name: str) -> str:
    # Functions end at an unindented closing brace; nested blocks are indented.
    source = (ROOT / "scripts" / filename).read_text()
    pattern = rf"(?ms)^(?:function )?{re.escape(name)}(?:\(\))? \{{.*?^\}}"
    match = re.search(pattern, source)
    if not match:
        raise AssertionError(f"installer function {name} not found in {filename}")
    return match[0]


class InstallerBootstrapTest(unittest.TestCase):
    def exercise(self, shell: str) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            calls = directory / "calls.jsonl"
            if shell == "pwsh":
                wrapper = directory / "fake wrapper.ps1"
                wrapper.write_text(
                    "$Call = ConvertTo-Json -InputObject @($args) -Compress\n"
                    "Add-Content -LiteralPath $env:BOOTSTRAP_CALLS -Value $Call\n"
                    'if ($args[-1] -eq "--help") {\n'
                    "  Write-Output $env:BOOTSTRAP_HELP\n"
                    "  exit [int]$env:BOOTSTRAP_HELP_EXIT\n"
                    "}\n"
                    "exit [int]$env:BOOTSTRAP_USE_EXIT\n"
                )
                script = directory / "bootstrap.ps1"
                script.write_text(
                    "param([string]$WrapperPath, [string]$Selector)\n"
                    + installer_function("install.ps1", "Invoke-BamlToolchainUse")
                    + "\nInvoke-BamlToolchainUse $WrapperPath $Selector\nexit $LASTEXITCODE\n"
                )
                command = [
                    shell,
                    "-NoProfile",
                    "-File",
                    str(script),
                    str(wrapper),
                    SELECTOR,
                ]
            else:
                wrapper = directory / "fake wrapper"
                wrapper.write_text(
                    f"#!{sys.executable}\n"
                    "import json, os, sys\n"
                    "with open(os.environ['BOOTSTRAP_CALLS'], 'a') as log:\n"
                    "    log.write(json.dumps(sys.argv[1:]) + '\\n')\n"
                    "if sys.argv[-1] == '--help':\n"
                    "    print(os.environ['BOOTSTRAP_HELP'])\n"
                    "    sys.exit(int(os.environ['BOOTSTRAP_HELP_EXIT']))\n"
                    "sys.exit(int(os.environ['BOOTSTRAP_USE_EXIT']))\n"
                )
                wrapper.chmod(0o755)
                script = directory / "bootstrap.sh"
                script.write_text(
                    "set -eu\n"
                    + installer_function("install.sh", "bootstrap_toolchain")
                    + '\nbootstrap_toolchain "$1" "$2"\n'
                )
                command = [shell, str(script), str(wrapper), SELECTOR]

            for name, help_text, scope, help_exit, use_exit in CASES:
                with self.subTest(shell=shell, generation=name):
                    calls.unlink(missing_ok=True)
                    result = subprocess.run(
                        command,
                        env=dict(
                            os.environ,
                            BOOTSTRAP_CALLS=str(calls),
                            BOOTSTRAP_HELP=help_text,
                            BOOTSTRAP_HELP_EXIT=str(help_exit),
                            BOOTSTRAP_USE_EXIT=str(use_exit),
                        ),
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    self.assertEqual(
                        result.returncode, help_exit or use_exit, result.stderr
                    )
                    expected = [["toolchain", "use", "--help"]]
                    if not help_exit:
                        expected.append(
                            ["toolchain", "use", SELECTOR] + ([scope] if scope else [])
                        )
                    self.assertEqual(
                        [
                            json.loads(line)
                            for line in calls.read_text(
                                encoding="utf-8-sig"
                            ).splitlines()
                        ],
                        expected,
                        "bootstrap must negotiate scope and invoke use at most once",
                    )

    @unittest.skipIf(os.name == "nt", "POSIX installer runs on Unix")
    def test_posix_bootstrap_negotiates_scope_without_retrying_failures(self) -> None:
        self.exercise("sh")

    @unittest.skipUnless(shutil.which("pwsh"), "PowerShell is not installed")
    def test_powershell_bootstrap_negotiates_scope_without_retrying_failures(
        self,
    ) -> None:
        self.exercise("pwsh")


if __name__ == "__main__":
    unittest.main()
