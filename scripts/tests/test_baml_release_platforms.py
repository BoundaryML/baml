from __future__ import annotations

import copy
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from scripts.baml_release_platforms import (
    DEFAULT_PLATFORMS,
    expected_wrapper_assets,
    load_wrapper_targets,
    verify_wrapper_artifacts,
)


VERSION = "1.2.3"
ROOT = Path(__file__).resolve().parents[2]
CLI = ROOT / "scripts" / "baml-release-platforms"
CSHARP_CLI = ROOT / "scripts" / "baml-csharp-release-contract"
RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "release-baml-language.yml"
TOOLCHAIN_WORKFLOW = ROOT / ".github" / "workflows" / "build2-toolchain.reusable.yaml"


class WrapperReleasePlatformTests(unittest.TestCase):
    def test_release_build_runner_regressions_are_pinned(self) -> None:
        contract = json.loads(DEFAULT_PLATFORMS.read_text(encoding="utf-8"))
        targets = {target["triple"]: target for target in contract["targets"]}

        arm64_macos = targets["aarch64-apple-darwin"]["artifacts"]
        self.assertEqual(
            {
                arm64_macos["toolchain"]["runner"],
                arm64_macos["wrapper"]["runner"],
                arm64_macos["java"]["runner"],
                arm64_macos["cffi"]["runner"],
                arm64_macos["csharp"]["consumer_runner"],
            },
            {"blacksmith-6vcpu-macos-latest"},
        )

        arm64_gnu = targets["aarch64-unknown-linux-gnu"]["artifacts"]["toolchain"]
        self.assertEqual(arm64_gnu["runner"], "ubuntu-latest")
        self.assertEqual(
            arm64_gnu["cross_image"],
            "ghcr.io/rust-cross/manylinux_2_28-cross:aarch64",
        )

        csharp_matrix = json.loads(
            subprocess.run(
                [CSHARP_CLI, "matrix"],
                check=True,
                capture_output=True,
                text=True,
            ).stdout
        )["include"]
        osx_arm64 = next(
            entry for entry in csharp_matrix if entry["rid"] == "osx-arm64"
        )
        self.assertEqual(osx_arm64["runner"], "blacksmith-6vcpu-macos-latest")

    def test_release_delegates_toolchain_build_with_legacy_glibc_compiler(self) -> None:
        release = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        builder = TOOLCHAIN_WORKFLOW.read_text(encoding="utf-8")

        self.assertIn(
            "uses: ./.github/workflows/build2-toolchain.reusable.yaml", release
        )
        self.assertNotIn("name: Build CLI and pack host", release)
        self.assertIn("CC_x86_64_unknown_linux_gnu=gcc", builder)
        self.assertIn(
            "CFLAGS_x86_64_unknown_linux_gnu=--sysroot=/usr/x86_64-unknown-linux-gnu/x86_64-unknown-linux-gnu/sysroot",
            builder,
        )

    def test_extensionless_cli_generates_wrapper_matrix(self) -> None:
        result = subprocess.run(
            [sys.executable, CLI, "wrapper-matrix"],
            check=True,
            capture_output=True,
            text=True,
        )

        self.assertEqual(
            json.loads(result.stdout),
            [target.matrix_entry() for target in load_wrapper_targets()],
        )

    def test_wrapper_matrix_carries_variants_and_platform_suffixes(self) -> None:
        targets = {target.triple: target for target in load_wrapper_targets()}

        self.assertEqual(len(targets), 8)
        for triple in (
            "aarch64-apple-darwin",
            "x86_64-apple-darwin",
            "aarch64-unknown-linux-gnu",
            "x86_64-unknown-linux-gnu",
            "aarch64-pc-windows-msvc",
            "x86_64-pc-windows-msvc",
        ):
            self.assertTrue(targets[triple].supports("no-self-update"))
        for triple in (
            "aarch64-unknown-linux-musl",
            "x86_64-unknown-linux-musl",
        ):
            self.assertFalse(targets[triple].supports("no-self-update"))

        windows = targets["x86_64-pc-windows-msvc"].matrix_entry()
        self.assertEqual(windows["archive_suffix"], ".zip")
        self.assertEqual(windows["executable_suffix"], ".exe")
        self.assertTrue(windows["no_self_update"])

    def test_expected_assets_include_both_windows_variants(self) -> None:
        assets = set(expected_wrapper_assets(VERSION))

        for triple in (
            "aarch64-pc-windows-msvc",
            "x86_64-pc-windows-msvc",
        ):
            self.assertIn(f"baml-wrapper-{VERSION}-{triple}.zip", assets)
            self.assertIn(
                f"baml-wrapper-no-self-update-{VERSION}-{triple}.zip",
                assets,
            )
        self.assertNotIn(
            f"baml-wrapper-no-self-update-{VERSION}-x86_64-unknown-linux-musl.tar.gz",
            assets,
        )

    def test_wrapper_artifact_verification_requires_exact_contract(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            wrapper_dir = Path(temporary)
            for asset in expected_wrapper_assets(VERSION):
                (wrapper_dir / asset).write_bytes(asset.encode())
            verify_wrapper_artifacts(wrapper_dir, VERSION)

            missing = (
                wrapper_dir
                / f"baml-wrapper-no-self-update-{VERSION}-x86_64-pc-windows-msvc.zip"
            )
            missing.unlink()
            with self.assertRaisesRegex(
                SystemExit, "wrapper artifact mismatch missing="
            ):
                verify_wrapper_artifacts(wrapper_dir, VERSION)

            missing.write_bytes(b"restored")
            unexpected = wrapper_dir / f"baml-wrapper-{VERSION}-unexpected.zip"
            unexpected.write_bytes(b"unexpected")
            with self.assertRaisesRegex(SystemExit, "extra=.*unexpected"):
                verify_wrapper_artifacts(wrapper_dir, VERSION)

    def test_wrapper_contract_rejects_invalid_metadata(self) -> None:
        original = json.loads(DEFAULT_PLATFORMS.read_text(encoding="utf-8"))
        cases = (
            (
                "unknown wrapper variants",
                lambda contract: contract["targets"][0]["artifacts"]["wrapper"][
                    "variants"
                ].append("unknown"),
            ),
            (
                "must include 'self-update'",
                lambda contract: contract["targets"][0]["artifacts"]["wrapper"].update(
                    variants=["no-self-update"]
                ),
            ),
            (
                "archive_suffix must be '.tar.gz'",
                lambda contract: contract["targets"][0].update(archive_suffix=".zip"),
            ),
            (
                "wrapper runner .* does not match macos",
                lambda contract: contract["targets"][0]["artifacts"]["wrapper"].update(
                    runner="windows-latest"
                ),
            ),
        )
        for message, mutate in cases:
            with self.subTest(message=message), tempfile.TemporaryDirectory() as temp:
                contract = copy.deepcopy(original)
                mutate(contract)
                platforms = Path(temp) / "platforms.json"
                platforms.write_text(json.dumps(contract), encoding="utf-8")
                with self.assertRaisesRegex(SystemExit, message):
                    load_wrapper_targets(platforms)


if __name__ == "__main__":
    unittest.main()
