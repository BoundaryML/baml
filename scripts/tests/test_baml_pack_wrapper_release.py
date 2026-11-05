from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile

from baml_language.wrapper_tests.release_artifacts import candidates_from_archives
from scripts.baml_release_platforms import (
    DEFAULT_PLATFORMS,
    expected_wrapper_assets,
    load_wrapper_targets,
    verify_wrapper_artifacts,
)


ROOT = Path(__file__).resolve().parents[2]
VERSION = "9.8.7"


class WrapperReleasePackingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.toolchains = self.root / "toolchains"
        self.output = self.root / "published"
        self.environment = {
            **os.environ,
            "BAML_WRAPPER_BOUNDARY_API_KEY": "test-release-key",
            "BAML_WRAPPER_BCS_PROJECT": "test-org/wrapper",
            "BAML_WRAPPER_BCS_ENVIRONMENT": "Test",
        }
        for target in load_wrapper_targets():
            directory = self.toolchains / f"toolchain-{target.triple}/unpacked/bin"
            directory.mkdir(parents=True)
            (directory / f"baml-pack-host{target.executable_suffix}").write_text(target.triple)
        cli = self.toolchains / "toolchain-x86_64-unknown-linux-gnu/unpacked/bin/baml-cli"
        # Exercise the real wrapper builder and archive writer, substituting only
        # the compiler so every target can be checked without executing foreign code.
        cli.write_text(
            f"#!{sys.executable}\n"
            "import json, pathlib, sys, tomllib\n"
            "args = sys.argv[1:]\n"
            "assert args[0] == 'pack' and '--embed-telemetry' in args\n"
            "project = pathlib.Path(args[args.index('--from') + 1])\n"
            "manifest = tomllib.loads((project / 'baml.toml').read_text())\n"
            "target = args[args.index('--target') + 1]\n"
            "host = pathlib.Path(args[args.index('--host-binary') + 1])\n"
            "assert host.read_text() == target\n"
            "payload = dict(target=target, project=manifest['boundary']['project'],\n"
            "    environment=manifest['pack']['build']['embed_telemetry_env'],\n"
            "    key_variable=manifest['pack']['build']['embed_telemetry_api_key_from_env_var'],\n"
            "    source=(project / 'baml_src/build.baml').read_text())\n"
            "pathlib.Path(args[args.index('--output') + 1]).write_text(json.dumps(payload))\n"
        )
        cli.chmod(0o755)

    def pack(self, *, check=True):
        return subprocess.run(
            [sys.executable, ROOT / "scripts/baml-pack-wrapper-release",
             "--toolchains", self.toolchains, "--out", self.output, "--version", VERSION],
            env=self.environment, capture_output=True, text=True, check=check,
        )

    def test_packs_all_canonical_archives_with_correct_hosts_and_variants(self):
        self.pack()
        self.assertEqual(
            {path.name for path in self.output.iterdir()},
            set(expected_wrapper_assets(VERSION)),
        )
        for target in load_wrapper_targets():
            for variant in target.variants:
                archive = self.output / target.asset_name(VERSION, variant)
                binary_name = f"bin/baml{target.executable_suffix}"
                with self.subTest(archive=archive.name):
                    if archive.suffix == ".zip":
                        with zipfile.ZipFile(archive) as source:
                            self.assertEqual(set(source.namelist()), {binary_name, "LICENSE"})
                            binary = source.read(binary_name)
                            license_text = source.read("LICENSE")
                    else:
                        with tarfile.open(archive) as source:
                            binary_info = source.getmember(f"./{binary_name}")
                            self.assertEqual(binary_info.mode & 0o777, 0o755)
                            binary = source.extractfile(binary_info).read()
                            license_text = source.extractfile("./LICENSE").read()
                    payload = json.loads(binary)
                    self.assertEqual(payload["target"], target.triple)
                    self.assertEqual(payload["project"], "test-org/wrapper")
                    self.assertEqual(payload["environment"], "Test")
                    self.assertEqual(payload["key_variable"], "BAML_WRAPPER_BOUNDARY_API_KEY")
                    self.assertIn(f'"{VERSION}"', payload["source"])
                    enabled = "true" if variant == "self-update" else "false"
                    self.assertIn(f"function self_update_enabled() -> bool {{\n    {enabled}\n}}", payload["source"])
                    self.assertEqual(license_text, (ROOT / "LICENSE").read_bytes())

    def test_missing_host_fails_before_producing_any_archives(self):
        host = next(self.toolchains.rglob("baml-pack-host.exe"))
        host.unlink()
        result = self.pack(check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing release toolchain executable", result.stderr)
        self.assertFalse(self.output.exists())

    def test_missing_release_configuration_fails_before_packing(self):
        self.environment["BAML_WRAPPER_BCS_PROJECT"] = " "
        result = self.pack(check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing wrapper BCS release configuration: BAML_WRAPPER_BCS_PROJECT", result.stderr)
        self.assertFalse(self.output.exists())

    def test_release_tests_stage_baml_canonical_and_rust_reference_separately(self):
        self.pack()
        platform = next(
            target for target in json.loads(DEFAULT_PLATFORMS.read_text())["targets"]
            if target["triple"] == "x86_64-pc-windows-msvc"
        )
        references = self.root / "rust"
        references.mkdir()
        for variant in ("", "-no-self-update"):
            archive = references / f"baml-wrapper-rust{variant}-{VERSION}-{platform['triple']}.zip"
            with zipfile.ZipFile(archive, "w") as output:
                output.writestr("bin/baml.exe", f"rust{variant}")
        staged = self.root / "staged"
        staged.mkdir()
        candidates = candidates_from_archives(self.root, staged, platform, VERSION)
        self.assertEqual({entry["label"] for entry in candidates}, {
            "baml", "baml-no-self-update", "rust", "rust-no-self-update",
        })
        for entry in candidates:
            binary = Path(entry["command"][0]).read_bytes()
            if entry["label"].startswith("rust"):
                self.assertEqual(binary.decode(), entry["label"])
            else:
                self.assertEqual(json.loads(binary)["target"], platform["triple"])
        canonical = self.output / f"baml-wrapper-{VERSION}-{platform['triple']}.zip"
        canonical.unlink()
        with self.assertRaisesRegex(ValueError, "expected exactly one baml-wrapper-.*found 0"):
            candidates_from_archives(self.root, staged, platform, VERSION)

    def test_publishing_verifier_rejects_rust_reference_archives(self):
        self.pack()
        archive = next(self.output.iterdir())
        archive.rename(archive.with_name(archive.name.replace("baml-wrapper-", "baml-wrapper-rust-", 1)))
        with self.assertRaisesRegex(SystemExit, "wrapper artifact mismatch missing=.*extra=.*rust"):
            verify_wrapper_artifacts(self.output, VERSION)


if __name__ == "__main__":
    unittest.main()
