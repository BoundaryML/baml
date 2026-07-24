from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MANIFEST_TOOL = ROOT / "scripts" / "baml-release-manifests"
PLATFORMS = ROOT / "release" / "platforms.json"


class ReleaseManifestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.toolchain = self.root / "toolchain"
        self.cffi = self.root / "cffi"
        self.wrapper = self.root / "wrapper"
        self.vsix = self.root / "vsix"
        for directory in (self.toolchain, self.cffi, self.wrapper, self.vsix):
            directory.mkdir()

        contract = json.loads(PLATFORMS.read_text(encoding="utf-8"))
        self.required_targets = {
            target["triple"]
            for target in contract["targets"]
            if target["artifacts"].get("toolchain") is not None
            and not target["artifacts"]["toolchain"].get("experimental", False)
        }
        for target in contract["targets"]:
            triple = target["triple"]
            suffix = ".zip" if target["os"] == "windows" else ".tar.gz"
            (self.toolchain / f"baml-language-1.2.3-{triple}{suffix}").write_bytes(
                triple.encode()
            )
            (self.wrapper / f"baml-wrapper-9.8.7-{triple}{suffix}").write_bytes(
                triple.encode()
            )
            cffi = target["artifacts"].get("cffi")
            if cffi is not None:
                (self.cffi / cffi["asset"]).write_bytes(triple.encode())
        (self.vsix / "baml-language-1.2.3.vsix").write_bytes(b"vsix")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def run_tool(
        self,
        output: Path,
        *extra: str,
        check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                str(MANIFEST_TOOL),
                "--toolchain-dir",
                str(self.toolchain),
                "--cffi-dir",
                str(self.cffi),
                "--wrapper-dir",
                str(self.wrapper),
                "--vsix-dir",
                str(self.vsix),
                "--out",
                str(output),
                "--channel",
                "canary",
                "--version",
                "1.2.3",
                "--released-at",
                "2026-07-23T12:34:56Z",
                "--pypi-version",
                "1.2.3",
                "--wrapper-version",
                "9.8.7",
                *extra,
            ],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=check,
        )

    def test_csharp_digest_and_registry_identity_are_in_dry_run_shape(self) -> None:
        digest = "a" * 64
        first = self.root / "first"
        second = self.root / "second"
        extra = (
            "--crates-io-version",
            "1.2.3",
            "--nuget-version",
            "1.2.3",
            "--nuget-package-sha256",
            digest,
            "--swiftpm-version",
            "1.2.3",
            "--swift-package-sha256",
            digest,
        )
        self.run_tool(first, *extra)
        self.run_tool(second, *extra)
        first_manifest = first / "version" / "1.2.3.json"
        second_manifest = second / "version" / "1.2.3.json"
        self.assertEqual(first_manifest.read_bytes(), second_manifest.read_bytes())
        manifest = json.loads(first_manifest.read_text(encoding="utf-8"))
        self.assertEqual(manifest["released_at"], "2026-07-23T12:34:56Z")
        self.assertEqual(
            manifest["sdks"]["csharp"],
            {
                "registry": "nuget",
                "package": "baml-bridge",
                "version": "1.2.3",
                "verified_package_sha256": digest,
            },
        )
        self.assertEqual(manifest["sdks"]["rust"]["registry"], "crates_io")
        self.assertEqual(
            manifest["sdks"]["swift"],
            {
                "registry": "swiftpm",
                "package": "BoundaryML/baml-swift",
                "version": "1.2.3",
                "verified_package_sha256": digest,
            },
        )

    def test_wrapper_manifest_can_be_generated_independently(self) -> None:
        output = self.root / "wrapper-only"
        subprocess.run(
            [
                str(MANIFEST_TOOL),
                "--wrapper-only",
                "--wrapper-dir",
                str(self.wrapper),
                "--out",
                str(output),
                "--released-at",
                "2026-07-23T12:34:56Z",
                "--wrapper-version",
                "9.8.7",
            ],
            cwd=ROOT,
            check=True,
        )
        manifest = json.loads((output / "wrapper.json").read_text(encoding="utf-8"))
        self.assertEqual(manifest["version"], "9.8.7")
        self.assertEqual(set(manifest["artifacts"]), self.required_targets)

    def test_partial_or_invalid_native_sdk_identity_fails(self) -> None:
        cases = (
            ("--nuget-version", "1.2.3"),
            ("--nuget-package-sha256", "a" * 64),
            (
                "--nuget-version",
                "1.2.3",
                "--nuget-package-sha256",
                "not-a-digest",
            ),
            ("--swiftpm-version", "1.2.3"),
            ("--swift-package-sha256", "a" * 64),
            (
                "--swiftpm-version",
                "1.2.3",
                "--swift-package-sha256",
                "not-a-digest",
            ),
        )
        for index, extra in enumerate(cases):
            result = self.run_tool(
                self.root / f"invalid-{index}",
                *extra,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
