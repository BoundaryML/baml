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
            ("Linux", "x86_64", "gnu", "x86_64-unknown-linux-gnu"),
            ("Linux", "x86_64", "musl", "x86_64-unknown-linux-musl"),
            ("Linux", "aarch64", "gnu", "aarch64-unknown-linux-gnu"),
            ("Linux", "aarch64", "musl", "aarch64-unknown-linux-musl"),
            ("Linux", "arm64", "musl", "aarch64-unknown-linux-musl"),
            ("Linux", "x86_64", "unknown", "x86_64-unknown-linux-musl"),
            ("Darwin", "x86_64", "", "x86_64-apple-darwin"),
            ("Darwin", "arm64", "", "aarch64-apple-darwin"),
        )

        for system, machine, libc, expected_target in cases:
            with (
                self.subTest(system=system, machine=machine, libc=libc),
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

                getconf = fake_bin / "getconf"
                getconf.write_text(
                    "#!/bin/sh\n"
                    'if [ "$FAKE_LIBC" = gnu ] && [ "${1:-}" = GNU_LIBC_VERSION ]; then\n'
                    "  printf '%s\\n' 'glibc 2.18'\n"
                    "  exit 0\n"
                    "fi\n"
                    "exit 1\n",
                    encoding="utf-8",
                )
                getconf.chmod(0o755)

                ldd = fake_bin / "ldd"
                ldd.write_text(
                    "#!/bin/sh\n"
                    'if [ "$FAKE_LIBC" = musl ]; then\n'
                    "  printf '%s\\n' 'musl libc (x86_64)' >&2\n"
                    "  exit 1\n"
                    "fi\n"
                    'if [ "$FAKE_LIBC" = gnu ]; then\n'
                    "  printf '%s\\n' 'ldd (GNU libc) 2.18'\n"
                    "  exit 0\n"
                    "fi\n"
                    "exit 1\n",
                    encoding="utf-8",
                )
                ldd.chmod(0o755)

                env = os.environ.copy()
                env.update(
                    {
                        "BAML_HOME": str(baml_home),
                        "BAML_MANIFEST_BASE_URL": manifest_dir.as_uri(),
                        "FAKE_UNAME_SYSTEM": system,
                        "FAKE_UNAME_MACHINE": machine,
                        "FAKE_LIBC": libc,
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
