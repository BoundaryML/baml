from __future__ import annotations

import json
import io
import os
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
VERSION_SCRIPT = ROOT / "scripts" / "baml-language-version"
PLATFORM_SCRIPT = ROOT / "scripts" / "baml-release-platforms"
MANIFEST_SCRIPT = ROOT / "scripts" / "baml-release-manifests"


class ReleaseContractTests(unittest.TestCase):
    def run_command(
        self, *args: object, env: dict[str, str] | None = None
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(arg) for arg in args],
            cwd=ROOT,
            env={**os.environ, **(env or {})},
            check=True,
            capture_output=True,
            text=True,
        )

    def test_release_plan_freezes_source_versions_and_time(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            plan_path = Path(tmp) / "release-plan.json"
            released_at = "2026-07-14T22:00:00Z"
            self.run_command(
                VERSION_SCRIPT,
                "plan",
                "--channel",
                "canary",
                "--out",
                plan_path,
                env={"BAML_LANGUAGE_RELEASED_AT": released_at},
            )
            plan = json.loads(plan_path.read_text())
            self.assertEqual(plan["schema"], 2)
            self.assertEqual(plan["released_at"], released_at)
            self.assertEqual(
                plan["registry_versions"]["npm"], plan["canonical_version"]
            )
            self.assertEqual(
                plan["registry_versions"]["pypi"], plan["canonical_version"]
            )
            self.assertEqual(
                plan["source_sha"],
                self.run_command("git", "rev-parse", "HEAD").stdout.strip(),
            )

    def test_platform_contract_generates_every_release_matrix(self) -> None:
        self.run_command(PLATFORM_SCRIPT, "check")
        expected_targets = set(
            self.run_command(PLATFORM_SCRIPT, "targets").stdout.splitlines()
        )
        self.assertEqual(len(expected_targets), 8)
        for surface in (
            "node_build",
            "node_verify",
            "python_build",
            "toolchain",
            "wrapper",
        ):
            rows = json.loads(
                self.run_command(PLATFORM_SCRIPT, "matrix", surface).stdout
            )
            self.assertEqual({row["target"] for row in rows}, expected_targets)
        verify_rows = json.loads(
            self.run_command(PLATFORM_SCRIPT, "matrix", "node_verify").stdout
        )
        self.assertEqual(
            [row["target"] for row in verify_rows if row.get("verify_public_tag")],
            ["x86_64-unknown-linux-gnu"],
        )

    def test_node_support_contract_matches_esm_only_umbrella(self) -> None:
        platforms = json.loads((ROOT / "release" / "platforms.json").read_text())
        package = json.loads(
            (
                ROOT
                / "baml_language"
                / "sdks"
                / "nodejs"
                / "bridge_nodejs"
                / "package.json"
            ).read_text()
        )
        minimum_major = platforms["node_support"]["minimum_major"]
        self.assertEqual(package["engines"]["node"], f">={minimum_major}")
        self.assertEqual(package["type"], "module")
        self.assertEqual(set(package["exports"]["."]), {"types", "import"})

    def test_node_umbrella_tarball_requires_exact_optional_dependencies(self) -> None:
        platforms = json.loads((ROOT / "release" / "platforms.json").read_text())
        version = "1.2.3"
        package = json.loads(
            (
                ROOT
                / "baml_language"
                / "sdks"
                / "nodejs"
                / "bridge_nodejs"
                / "package.json"
            ).read_text()
        )
        package["version"] = version
        package["optionalDependencies"] = {
            f"@boundaryml/baml-bridge-{entry['npm_package']}": version
            for entry in platforms["platforms"]
        }
        with tempfile.TemporaryDirectory() as tmp:
            tarball = Path(tmp) / "umbrella.tgz"
            with tarfile.open(tarball, "w:gz") as archive:
                files = {
                    "package/package.json": json.dumps(package).encode(),
                    "package/dist/index.js": b"export {};\n",
                    "package/dist/index.d.ts": b"export {};\n",
                    "package/dist/native.js": b"export {};\n",
                    "package/dist/native.d.ts": b"export {};\n",
                }
                for name, payload in files.items():
                    info = tarfile.TarInfo(name)
                    info.size = len(payload)
                    archive.addfile(info, io.BytesIO(payload))
            self.run_command(
                PLATFORM_SCRIPT,
                "check-node-umbrella",
                tarball,
                "--version",
                version,
            )

            package["optionalDependencies"].pop(
                next(iter(package["optionalDependencies"]))
            )
            with tarfile.open(tarball, "w:gz") as archive:
                payload = json.dumps(package).encode()
                info = tarfile.TarInfo("package/package.json")
                info.size = len(payload)
                archive.addfile(info, io.BytesIO(payload))
                for name in (
                    "package/dist/index.js",
                    "package/dist/index.d.ts",
                    "package/dist/native.js",
                    "package/dist/native.d.ts",
                ):
                    info = tarfile.TarInfo(name)
                    info.size = 0
                    archive.addfile(info, io.BytesIO())
            failed = subprocess.run(
                [
                    str(PLATFORM_SCRIPT),
                    "check-node-umbrella",
                    str(tarball),
                    "--version",
                    version,
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(failed.returncode, 0)

    def test_aarch64_musl_cross_toolchain_is_verified_before_extraction(self) -> None:
        expected_sha256 = (
            "c909817856d6ceda86aa510894fa3527eac7989f0ef6e87b5721c58737a06c38"
        )
        rows = json.loads(
            self.run_command(PLATFORM_SCRIPT, "matrix", "node_build").stdout
        )
        row = next(
            row
            for row in rows
            if row["target"] == "aarch64-unknown-linux-musl"
        )
        self.assertEqual(row["cross_toolchain_sha256"], expected_sha256)
        self.assertIn(f"expected_sha256={expected_sha256}", row["before"])
        self.assertLess(
            row["before"].index("sha256sum --check --strict"),
            row["before"].index("tar -xzf"),
        )

    def test_node_package_set_requires_exact_names_versions_and_budgets(self) -> None:
        platforms = json.loads((ROOT / "release" / "platforms.json").read_text())
        names = ["@boundaryml/baml-bridge"] + [
            f"@boundaryml/baml-bridge-{entry['npm_package']}"
            for entry in platforms["platforms"]
        ]
        packages = [
            {
                "name": name,
                "version": "1.2.3",
                "filename": f"package-{index}.tgz",
                "size": 100,
                "unpackedSize": 100,
                "integrity": f"sha512-test-{index}",
            }
            for index, name in enumerate(names)
        ]
        with tempfile.TemporaryDirectory() as tmp:
            package_path = Path(tmp) / "packages.json"
            package_path.write_text(json.dumps(packages))
            self.run_command(
                PLATFORM_SCRIPT,
                "check-node-packages",
                package_path,
                "--version",
                "1.2.3",
            )
            packages[0]["version"] = "9.9.9"
            package_path.write_text(json.dumps(packages))
            failed = subprocess.run(
                [
                    str(PLATFORM_SCRIPT),
                    "check-node-packages",
                    str(package_path),
                    "--version",
                    "1.2.3",
                ],
                cwd=ROOT,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(failed.returncode, 0)

    def test_manifest_generation_is_deterministic_and_records_sdks(self) -> None:
        platforms = json.loads((ROOT / "release" / "platforms.json").read_text())
        version = "1.2.3-nightly.20260714.a"
        plan = {
            "schema": 2,
            "channel": "nightly",
            "canary_version": "1.2.2",
            "canonical_version": version,
            "registry_versions": {"npm": version, "pypi": "1.2.3.dev2026071400"},
            "git_tag": f"baml-language-{version}",
            "source_sha": "0" * 40,
            "released_at": "2026-07-14T22:00:00Z",
        }
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            toolchain = root / "toolchain"
            wrapper = root / "wrapper"
            vsix = root / "vsix"
            toolchain.mkdir()
            wrapper.mkdir()
            vsix.mkdir()
            for entry in platforms["platforms"]:
                target = entry["target"]
                extension = "zip" if target.endswith("windows-msvc") else "tar.gz"
                (
                    toolchain / f"baml-language-{version}-{target}.{extension}"
                ).write_bytes(target.encode())
            (vsix / f"baml-language-{version}.vsix").write_bytes(b"vsix")
            plan_path = root / "release-plan.json"
            plan_path.write_text(json.dumps(plan))

            outputs = []
            for name in ("one", "two"):
                out = root / name
                self.run_command(
                    MANIFEST_SCRIPT,
                    "--plan",
                    plan_path,
                    "--toolchain-dir",
                    toolchain,
                    "--wrapper-dir",
                    wrapper,
                    "--vsix-dir",
                    vsix,
                    "--out",
                    out,
                    "--wrapper-version",
                    "1.0.0",
                )
                outputs.append((out / "version" / f"{version}.json").read_bytes())
            self.assertEqual(outputs[0], outputs[1])
            manifest = json.loads(outputs[0])
            self.assertEqual(manifest["released_at"], plan["released_at"])
            self.assertEqual(
                manifest["sdks"]["nodejs"]["package"], "@boundaryml/baml-bridge"
            )
            self.assertEqual(manifest["sdks"]["nodejs"]["version"], version)

    def test_manifest_generation_requires_exactly_the_versioned_vsix(self) -> None:
        platforms = json.loads((ROOT / "release" / "platforms.json").read_text())
        version = "1.2.3-nightly.20260714.a"
        plan = {
            "schema": 2,
            "channel": "nightly",
            "canary_version": "1.2.2",
            "canonical_version": version,
            "registry_versions": {"npm": version, "pypi": "1.2.3.dev2026071400"},
            "git_tag": f"baml-language-{version}",
            "source_sha": "0" * 40,
            "released_at": "2026-07-14T22:00:00Z",
        }
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            toolchain = root / "toolchain"
            wrapper = root / "wrapper"
            vsix = root / "vsix"
            toolchain.mkdir()
            wrapper.mkdir()
            vsix.mkdir()
            for entry in platforms["platforms"]:
                target = entry["target"]
                extension = "zip" if target.endswith("windows-msvc") else "tar.gz"
                (
                    toolchain / f"baml-language-{version}-{target}.{extension}"
                ).write_bytes(target.encode())
            plan_path = root / "release-plan.json"
            plan_path.write_text(json.dumps(plan))

            def run_manifest(out: str) -> subprocess.CompletedProcess[str]:
                return subprocess.run(
                    [
                        str(MANIFEST_SCRIPT),
                        "--plan",
                        str(plan_path),
                        "--toolchain-dir",
                        str(toolchain),
                        "--wrapper-dir",
                        str(wrapper),
                        "--vsix-dir",
                        str(vsix),
                        "--out",
                        str(root / out),
                        "--wrapper-version",
                        "1.0.0",
                    ],
                    cwd=ROOT,
                    capture_output=True,
                    text=True,
                )

            wrong_name = vsix / "extension.vsix"
            wrong_name.write_bytes(b"vsix")
            missing = run_manifest("missing")
            self.assertNotEqual(missing.returncode, 0)
            self.assertIn(f"missing=['baml-language-{version}.vsix']", missing.stderr)
            self.assertIn("extra=['extension.vsix']", missing.stderr)

            expected = vsix / f"baml-language-{version}.vsix"
            expected.write_bytes(b"vsix")
            extra = run_manifest("extra")
            self.assertNotEqual(extra.returncode, 0)
            self.assertIn("extra=['extension.vsix']", extra.stderr)


if __name__ == "__main__":
    unittest.main()
