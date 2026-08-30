from __future__ import annotations

import hashlib
import json
import os
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
INSTALLER = ROOT / "scripts" / "install.sh"


class InstallShTests(unittest.TestCase):
    def test_platforms_select_bootstrap_compatible_wrappers_offline(self) -> None:
        """Verify that every supported Unix host selects its bootstrap-safe wrapper."""
        cases = (
            ("Linux", "x86_64", "x86_64-unknown-linux-musl"),
            ("Linux", "aarch64", "aarch64-unknown-linux-musl"),
            ("Linux", "arm64", "aarch64-unknown-linux-musl"),
            ("Darwin", "x86_64", "x86_64-apple-darwin"),
            ("Darwin", "arm64", "aarch64-apple-darwin"),
        )

        for system, machine, expected_target in cases:
            with (
                self.subTest(system=system, machine=machine),
                tempfile.TemporaryDirectory() as temporary,
            ):
                temp = Path(temporary)
                manifest_dir = temp / "manifest" / "v1"
                staging_bin = temp / "staging" / "bin"
                fake_bin = temp / "fake-bin"
                baml_home = temp / "baml-home"
                manifest_dir.mkdir(parents=True)
                staging_bin.mkdir(parents=True)
                fake_bin.mkdir()

                wrapper = staging_bin / "baml"
                wrapper.write_text(
                    f"#!/bin/sh\n# {expected_target}\nexit 0\n", encoding="utf-8"
                )
                wrapper.chmod(0o755)
                archive = temp / "wrapper.tar.gz"
                with tarfile.open(archive, "w:gz") as tar:
                    tar.add(wrapper, arcname="bin/baml")

                digest = hashlib.sha256(archive.read_bytes()).hexdigest()
                (manifest_dir / "wrapper.json").write_text(
                    json.dumps(
                        {
                            "schema": 1,
                            "version": "test",
                            "released_at": "2026-08-28T00:00:00Z",
                            "artifacts": {
                                expected_target: {
                                    "url": archive.as_uri(),
                                    "sha256": digest,
                                }
                            },
                        }
                    ),
                    encoding="utf-8",
                )

                uname = fake_bin / "uname"
                uname.write_text(
                    "#!/bin/sh\n"
                    'case "$1" in\n'
                    "  -s) printf '%s\\n' \"$FAKE_UNAME_SYSTEM\" ;;\n"
                    "  -m) printf '%s\\n' \"$FAKE_UNAME_MACHINE\" ;;\n"
                    "  *) exit 1 ;;\n"
                    "esac\n",
                    encoding="utf-8",
                )
                uname.chmod(0o755)

                env = os.environ.copy()
                env.update(
                    {
                        "BAML_HOME": str(baml_home),
                        "BAML_MANIFEST_BASE_URL": manifest_dir.as_uri(),
                        "FAKE_UNAME_SYSTEM": system,
                        "FAKE_UNAME_MACHINE": machine,
                        "HOME": str(temp / "home"),
                        "PATH": f"{fake_bin}{os.pathsep}{env['PATH']}",
                    }
                )
                result = subprocess.run(
                    ["sh", str(INSTALLER), "--wrapper-only", "--no-modify-path"],
                    cwd=ROOT,
                    env=env,
                    check=True,
                    capture_output=True,
                    text=True,
                )

                installed = baml_home / "bin" / "baml"
                self.assertIn(expected_target, installed.read_text(encoding="utf-8"))
                self.assertIn(f"BAML installed at {installed}", result.stdout)


if __name__ == "__main__":
    unittest.main()
