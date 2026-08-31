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


class WrapperReleasePlatformTests(unittest.TestCase):
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
        platform_targets = {
            target["triple"]: target
            for target in json.loads(DEFAULT_PLATFORMS.read_text(encoding="utf-8"))[
                "targets"
            ]
        }

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

        cross_images = {
            "aarch64-unknown-linux-gnu": "ghcr.io/rust-cross/manylinux2014-cross:aarch64",
            "x86_64-unknown-linux-gnu": "ghcr.io/rust-cross/manylinux2014-cross:x86_64",
        }
        for triple, cross_image in cross_images.items():
            self.assertEqual(targets[triple].runner, "ubuntu-latest")
            self.assertEqual(targets[triple].cross_image, cross_image)
            self.assertEqual(targets[triple].matrix_entry()["cross_image"], cross_image)
            self.assertEqual(
                platform_targets[triple]["artifacts"]["toolchain"]["cross_image"],
                cross_image,
            )
        for triple, target in targets.items():
            if triple not in cross_images:
                self.assertIsNone(target.cross_image)
                self.assertNotIn("cross_image", target.matrix_entry())

    def test_expected_assets_include_both_linux_libcs_and_windows_variants(
        self,
    ) -> None:
        assets = set(expected_wrapper_assets(VERSION))

        for arch in ("aarch64", "x86_64"):
            for libc in ("gnu", "musl"):
                self.assertIn(
                    f"baml-wrapper-{VERSION}-{arch}-unknown-linux-{libc}.tar.gz",
                    assets,
                )
            self.assertIn(
                f"baml-wrapper-no-self-update-{VERSION}-{arch}-unknown-linux-gnu.tar.gz",
                assets,
            )
            self.assertNotIn(
                f"baml-wrapper-no-self-update-{VERSION}-{arch}-unknown-linux-musl.tar.gz",
                assets,
            )

        for triple in (
            "aarch64-pc-windows-msvc",
            "x86_64-pc-windows-msvc",
        ):
            self.assertIn(f"baml-wrapper-{VERSION}-{triple}.zip", assets)
            self.assertIn(
                f"baml-wrapper-no-self-update-{VERSION}-{triple}.zip",
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
            (
                "wrapper.cross_image must be a non-empty string",
                lambda contract: contract["targets"][2]["artifacts"]["wrapper"].pop(
                    "cross_image"
                ),
            ),
            (
                "wrapper.cross_image is only valid for GNU targets",
                lambda contract: contract["targets"][3]["artifacts"]["wrapper"].update(
                    cross_image="example.invalid/image@sha256:" + "0" * 64
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
