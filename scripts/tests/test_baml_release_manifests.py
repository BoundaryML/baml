from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "baml-release-manifests"
PLATFORMS = ROOT / "release" / "platforms.json"


class ReleaseManifestTests(unittest.TestCase):
    def test_records_published_rust_and_csharp_sdk_coordinates(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            toolchain = root / "toolchain"
            cffi = root / "cffi"
            wrapper = root / "wrapper"
            vsix = root / "vsix"
            out = root / "out"
            for directory in (toolchain, cffi, wrapper, vsix):
                directory.mkdir()

            contract = json.loads(PLATFORMS.read_text(encoding="utf-8"))
            targets = [
                target["triple"]
                for target in contract["targets"]
                if (
                    target["artifacts"].get("toolchain") is not None
                    and not target["artifacts"]["toolchain"].get("experimental", False)
                )
            ]
            version = "1.2.3-nightly.20260717.a"
            for target in targets:
                (toolchain / f"baml-language-{version}-{target}.zip").write_bytes(
                    f"toolchain:{target}".encode()
                )
                (wrapper / f"baml-wrapper-1.2.3-{target}.zip").write_bytes(
                    f"wrapper:{target}".encode()
                )
            (vsix / f"baml-language-{version}.vsix").write_bytes(b"vsix")

            subprocess.run(
                [
                    SCRIPT,
                    "--toolchain-dir",
                    toolchain,
                    "--cffi-dir",
                    cffi,
                    "--wrapper-dir",
                    wrapper,
                    "--vsix-dir",
                    vsix,
                    "--out",
                    out,
                    "--channel",
                    "nightly",
                    "--version",
                    version,
                    "--pypi-version",
                    "1.2.3.dev2026071700",
                    "--crates-io-version",
                    version,
                    "--nuget-version",
                    version,
                    "--wrapper-version",
                    "1.2.3",
                ],
                cwd=ROOT,
                check=True,
            )

            manifest = json.loads((out / "version" / f"{version}.json").read_text())
            self.assertEqual(
                manifest["sdks"],
                {
                    "csharp": {
                        "package": "baml-bridge",
                        "registry": "nuget_org",
                        "version": version,
                    },
                    "rust": {
                        "package": "baml_bridge",
                        "registry": "crates_io",
                        "version": version,
                    },
                },
            )


if __name__ == "__main__":
    unittest.main()
