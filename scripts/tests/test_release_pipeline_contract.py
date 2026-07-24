from __future__ import annotations

import copy
import hashlib
import json
import subprocess
import tempfile
import unittest
from collections.abc import Callable
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CONTRACT_TOOL = ROOT / "scripts" / "baml-csharp-release-contract"
PLATFORMS = ROOT / "release" / "platforms.json"
RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "release-baml-language.yml"
SIZE_POLICY = ROOT / "release" / "csharp-package-size-policy.json"
NUGET_PUBLISHER = ROOT / ".github" / "workflows" / "publish2-csharp-sdk.yaml"
CSHARP_VERIFIER = (
    ROOT / ".github" / "workflows" / "verify-csharp-product-slice.reusable.yaml"
)
CARGO_TESTS = ROOT / ".github" / "workflows" / "cargo-tests.reusable.yaml"
CFFI_BUILDER = ROOT / ".github" / "workflows" / "build2-bridge-cffi.reusable.yaml"
PACK_PRODUCT = (
    ROOT
    / "baml_language"
    / "sdks"
    / "csharp"
    / "bridge_csharp"
    / "tools"
    / "pack-product.sh"
)
NUGET_NORMALIZER = (
    ROOT
    / "baml_language"
    / "sdks"
    / "csharp"
    / "bridge_csharp"
    / "tools"
    / "Baml.NuGetNormalizer"
    / "Program.cs"
)


class CSharpReleaseContractTests(unittest.TestCase):
    maxDiff = None

    def run_tool(
        self,
        *args: str,
        check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(CONTRACT_TOOL), *args],
            cwd=ROOT,
            check=check,
            text=True,
            capture_output=True,
        )

    def write_contract(self, root: Path, contract: dict) -> Path:
        path = root / "platforms.json"
        path.write_text(json.dumps(contract), encoding="utf-8")
        return path

    def current_contract(self) -> dict:
        return json.loads(PLATFORMS.read_text(encoding="utf-8"))

    def test_exact_required_matrix(self) -> None:
        output = self.run_tool("matrix").stdout
        matrix = json.loads(output)["include"]
        actual = {
            (
                entry["target"],
                entry["rid"],
                entry["canonical"],
                entry["runner"],
            )
            for entry in matrix
        }
        expected = {
            (
                "aarch64-apple-darwin",
                "osx-arm64",
                "libbridge_cffi.dylib",
                "macos-14",
            ),
            (
                "x86_64-apple-darwin",
                "osx-x64",
                "libbridge_cffi.dylib",
                "macos-15-intel",
            ),
            (
                "aarch64-unknown-linux-gnu",
                "linux-arm64",
                "libbridge_cffi.so",
                "ubuntu-24.04-arm",
            ),
            (
                "x86_64-unknown-linux-gnu",
                "linux-x64",
                "libbridge_cffi.so",
                "ubuntu-24.04",
            ),
            (
                "aarch64-unknown-linux-musl",
                "linux-musl-arm64",
                "libbridge_cffi.so",
                "ubuntu-24.04-arm",
            ),
            (
                "x86_64-unknown-linux-musl",
                "linux-musl-x64",
                "libbridge_cffi.so",
                "ubuntu-24.04",
            ),
            (
                "aarch64-pc-windows-msvc",
                "win-arm64",
                "bridge_cffi.dll",
                "windows-11-arm",
            ),
            (
                "x86_64-pc-windows-msvc",
                "win-x64",
                "bridge_cffi.dll",
                "windows-2022",
            ),
        }
        self.assertEqual(actual, expected)
        size_policy = json.loads(SIZE_POLICY.read_text(encoding="utf-8"))
        self.assertEqual(
            set(size_policy["native_assets"]["baseline_bytes_by_target"]),
            {entry[0] for entry in expected},
        )
        self.assertLess(
            size_policy["compressed_package"]["baseline_bytes"],
            size_policy["compressed_package"]["registry_safety_ceiling_bytes"],
        )

    def test_invalid_or_missing_csharp_metadata_fails_early(self) -> None:
        cases: list[tuple[str, Callable[[dict], object]]] = [
            (
                "empty",
                lambda contract: [
                    target["artifacts"].pop("csharp", None)
                    for target in contract["targets"]
                ],
            ),
            (
                "duplicate-target",
                lambda contract: contract["targets"].append(
                    copy.deepcopy(contract["targets"][0])
                ),
            ),
            (
                "duplicate-rid",
                lambda contract: contract["targets"][1]["artifacts"]["csharp"].update(
                    {"rid": contract["targets"][0]["artifacts"]["csharp"]["rid"]}
                ),
            ),
            (
                "missing-runner",
                lambda contract: contract["targets"][0]["artifacts"]["csharp"].update(
                    {"consumer_runner": ""}
                ),
            ),
            (
                "missing-native-name",
                lambda contract: contract["targets"][0]["artifacts"]["csharp"].update(
                    {"native_asset": ""}
                ),
            ),
            (
                "optional-producer",
                lambda contract: contract["targets"][0]["artifacts"]["cffi"].update(
                    {"experimental": True}
                ),
            ),
        ]
        for name, mutate in cases:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                contract = self.current_contract()
                mutate(contract)
                path = self.write_contract(Path(temporary), contract)
                result = self.run_tool(
                    "validate",
                    "--platforms",
                    str(path),
                    check=False,
                )
                self.assertNotEqual(result.returncode, 0, result.stdout)

    def make_downloads(
        self,
        root: Path,
        *,
        missing_checksum_target: str | None = None,
        wrong_checksum_target: str | None = None,
        duplicate_target: str | None = None,
        include_unrelated: bool = True,
    ) -> Path:
        downloads = root / "downloads"
        contract = self.current_contract()
        for target in contract["targets"]:
            csharp = target["artifacts"].get("csharp")
            if csharp is None:
                continue
            triple = target["triple"]
            asset = target["artifacts"]["cffi"]["asset"]
            artifact = downloads / f"bridge-cffi-{triple}" / "nested"
            artifact.mkdir(parents=True)
            payload = f"native:{triple}".encode()
            (artifact / asset).write_bytes(payload)
            if triple != missing_checksum_target:
                digest = hashlib.sha256(payload).hexdigest()
                if triple == wrong_checksum_target:
                    digest = "0" * 64
                marker = " *" if "windows" in triple else "  "
                (artifact / f"{asset}.sha256").write_text(
                    f"{digest}{marker}{asset}\n",
                    encoding="utf-8",
                )
            if triple == duplicate_target:
                (artifact / "unexpected.txt").write_text("duplicate", encoding="utf-8")
        if include_unrelated:
            unrelated = downloads / "bridge-cffi-future-cffi-only"
            unrelated.mkdir(parents=True)
            (unrelated / "future.so").write_text("ignored", encoding="utf-8")
        return downloads

    def stage(
        self,
        root: Path,
        downloads: Path,
        *,
        check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        return self.run_tool(
            "stage-native",
            "--downloads",
            str(downloads),
            "--staging",
            str(root / "staging"),
            "--source-sha",
            "1" * 40,
            "--release-version",
            "1.2.3-nightly.20260723.a",
            "--workflow-run-id",
            "29989654236",
            "--provenance",
            str(root / "native-provenance.json"),
            "--manifest",
            str(root / "native-manifest.sha256"),
            check=check,
        )

    def write_plan(self, root: Path) -> Path:
        path = root / "release-plan.json"
        path.write_text(
            json.dumps(
                {
                    "schema": 2,
                    "canonical_version": "1.2.3-nightly.20260723.a",
                    "registry_versions": {
                        "nuget": "1.2.3-nightly.20260723.a",
                    },
                }
            ),
            encoding="utf-8",
        )
        return path

    def verify(
        self, root: Path, *, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        return self.run_tool(
            "verify-product",
            "--release-plan",
            str(self.write_plan(root)),
            "--source-sha",
            "1" * 40,
            "--provenance",
            str(root / "native-provenance.json"),
            "--manifest",
            str(root / "native-manifest.sha256"),
            "--native-root",
            str(root / "staging"),
            check=check,
        )

    def test_library_and_checksum_are_accepted_and_cffi_only_extra_is_ignored(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.stage(root, self.make_downloads(root))
            self.verify(root)
            provenance = json.loads(
                (root / "native-provenance.json").read_text(encoding="utf-8")
            )
            self.assertEqual(provenance["workflow_run_id"], "29989654236")
            self.assertNotIn("workflow_run_attempt", provenance)
            self.assertEqual(len(provenance["artifacts"]), 8)

    def test_missing_wrong_and_duplicate_producer_inputs_are_rejected(self) -> None:
        target = "x86_64-unknown-linux-gnu"
        options = (
            {"missing_checksum_target": target},
            {"wrong_checksum_target": target},
            {"duplicate_target": target},
        )
        for option in options:
            with (
                self.subTest(option=option),
                tempfile.TemporaryDirectory() as temporary,
            ):
                root = Path(temporary)
                result = self.stage(
                    root,
                    self.make_downloads(root, **option),
                    check=False,
                )
                self.assertNotEqual(result.returncode, 0)

    def test_wrong_source_version_target_and_attempt_binding_are_rejected(self) -> None:
        mutations = (
            lambda data: data.update({"source_sha": "2" * 40}),
            lambda data: data.update({"release_version": "9.9.9"}),
            lambda data: data["artifacts"][0].update({"target": "wrong-target"}),
            lambda data: data.update({"workflow_run_attempt": "2"}),
        )
        for mutate in mutations:
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                self.stage(root, self.make_downloads(root))
                path = root / "native-provenance.json"
                provenance = json.loads(path.read_text(encoding="utf-8"))
                mutate(provenance)
                path.write_text(json.dumps(provenance), encoding="utf-8")
                result = self.verify(root, check=False)
                self.assertNotEqual(result.returncode, 0)


def job_block(text: str, job: str) -> str:
    marker = f"  {job}:\n"
    start = text.index(marker)
    remainder = text[start + len(marker) :]
    next_job = next(
        (
            index
            for index, line in enumerate(remainder.splitlines(keepends=True))
            if line.startswith("  ")
            and not line.startswith("    ")
            and line.rstrip().endswith(":")
        ),
        None,
    )
    if next_job is None:
        return remainder
    lines = remainder.splitlines(keepends=True)
    return "".join(lines[:next_job])


class WorkflowGraphTests(unittest.TestCase):
    def test_release_graph_has_early_preflight_parallel_producers_and_complete_fanin(
        self,
    ) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        build_matrix = job_block(workflow, "build-matrix")
        prepare = job_block(workflow, "prepare-csharp-sdk")
        cffi = job_block(workflow, "build-bridge-cffi")
        verify = job_block(workflow, "verify-csharp-sdk")
        all_builds = job_block(workflow, "all-builds")
        wrapper_builds = job_block(workflow, "wrapper-builds")
        nuget = job_block(workflow, "publish-csharp-sdk")
        wrapper_release = job_block(workflow, "publish-wrapper-release")
        homebrew = job_block(workflow, "publish-homebrew")
        aur = job_block(workflow, "publish-aur")
        wrapper_manifest = job_block(workflow, "publish-wrapper-manifest")
        prerequisites = job_block(workflow, "release-prerequisites-complete")
        complete = job_block(workflow, "release-complete")
        manifest = job_block(workflow, "publish-pkg-boundaryml-com")
        go_publisher = job_block(workflow, "publish-go-sdk")
        channel = job_block(workflow, "publish-pkg-channel")
        dry_run = job_block(workflow, "dry-run-artifacts")

        self.assertIn("baml-csharp-release-contract matrix", build_matrix)
        self.assertIn("needs: [plan, build-matrix]", prepare)
        self.assertNotIn("build-bridge-cffi", prepare)
        self.assertIn("needs: [plan]", cffi)
        self.assertIn("prepare-csharp-sdk", verify)
        self.assertIn("build-bridge-cffi", verify)
        self.assertIn("- verify-csharp-sdk", all_builds)
        self.assertIn("all-builds", nuget)
        self.assertIn("- build-wrapper", wrapper_builds)
        self.assertNotIn("all-builds", wrapper_builds)
        for publisher in (wrapper_release, homebrew, aur):
            self.assertIn("wrapper-builds", publisher)
            self.assertNotIn("all-builds", publisher)
        self.assertIn("publish-wrapper-release", wrapper_manifest)
        self.assertIn("publish-homebrew", wrapper_manifest)
        self.assertIn("publish-aur", wrapper_manifest)
        self.assertIn("--wrapper-only", wrapper_manifest)
        for publisher in (
            "publish-pypi",
            "publish-nodejs-sdk",
            "publish-web-sdk",
            "publish-csharp-sdk",
            "publish-maven",
            "publish-gradle-plugin",
            "publish-toolchain-release",
            "publish-wrapper-release",
            "publish-bridge-cffi-release",
            "publish-swift-sdk",
            "publish-crates-io",
            "publish-homebrew",
            "publish-aur",
            "publish-wrapper-manifest",
        ):
            self.assertIn(f"- {publisher}", prerequisites)
        self.assertIn(
            "needs: [plan, release-prerequisites-complete]",
            manifest,
        )
        self.assertIn("publish-pkg-boundaryml-com", go_publisher)
        self.assertIn("release-prerequisites-complete", complete)
        self.assertIn("publish-go-sdk", complete)
        self.assertIn("release-complete", channel)
        self.assertIn("smoke-go-release", channel)
        self.assertIn("smoke-swift-release", channel)
        self.assertIn("--nuget-package-sha256", manifest)
        self.assertIn("name: swift-xcframework", manifest)
        self.assertIn("--swift-package-sha256", manifest)
        self.assertIn("name: csharp-product-package", dry_run)
        self.assertIn("--nuget-package-sha256", dry_run)
        self.assertIn("name: swift-xcframework", dry_run)
        self.assertIn("--swift-package-sha256", dry_run)
        for block in (manifest, dry_run):
            for output, variable in (
                ("crates_io_version", "CRATES_IO_VERSION"),
                ("nuget_version", "NUGET_VERSION"),
                ("swiftpm_version", "SWIFTPM_VERSION"),
                ("channel", "CHANNEL"),
                ("version", "VERSION"),
                ("released_at", "RELEASED_AT"),
                ("pypi_version", "PYPI_VERSION"),
                ("wrapper_version", "WRAPPER_VERSION"),
            ):
                self.assertIn(
                    f"{variable}: ${{{{ needs.plan.outputs.{output} }}}}",
                    block,
                )
        self.assertIn(
            "WRAPPER_CHANGED: ${{ needs.plan.outputs.wrapper_changed }}",
            dry_run,
        )
        self.assertNotIn("WRAPPER_CHANGED", manifest)

        swift_verify = job_block(workflow, "verify-swift-sdk")
        swift_smoke = job_block(workflow, "smoke-swift-release")
        dispatch = job_block(workflow, "dispatch-nightly-after-canary")
        self.assertIn("BamlRuntime.nativeVersion()", swift_verify)
        self.assertIn("BamlRuntime.nativeVersion()", swift_smoke)
        self.assertIn("publish-pkg-channel", dispatch)
        self.assertIn(
            '--cfg=getrandom_backend=\\"wasm_js\\"',
            CFFI_BUILDER.read_text(encoding="utf-8"),
        )

    def test_nuget_repair_and_nightly_pack_contracts_are_fail_closed(self) -> None:
        publisher = NUGET_PUBLISHER.read_text(encoding="utf-8")
        verifier = CSHARP_VERIFIER.read_text(encoding="utf-8")
        cargo_tests = CARGO_TESTS.read_text(encoding="utf-8")
        pack = PACK_PRODUCT.read_text(encoding="utf-8")
        normalizer = NUGET_NORMALIZER.read_text(encoding="utf-8")
        self.assertIn("404)", publisher)
        self.assertIn('echo "publish=true"', publisher)
        self.assertIn("200)", publisher)
        self.assertIn('compare "$PACKAGE"', publisher)
        self.assertIn(
            "if: ${{ steps.existing.outputs.publish == 'true' }}",
            publisher,
        )
        smoke = publisher.index(
            "- name: Restore and execute the exact public version from a clean cache"
        )
        self.assertNotIn("\n        if:", publisher[smoke : smoke + 300])

        self.assertIn(".registry_versions.nuget", pack)
        self.assertIn('-p:PackageVersion="$nuget_version"', pack)
        self.assertIn('-p:InformationalVersion="$canonical_version"', pack)
        self.assertNotIn('baml-language-version" show', pack)
        self.assertNotIn("workflow_run_attempt", pack)
        self.assertNotIn("baml-language-version show", verifier)
        self.assertIn(".registry_versions.nuget", verifier)
        self.assertIn('name: "Test nightly C# package version"', cargo_tests)
        self.assertIn('-p:PackageVersion="$nuget"', cargo_tests)
        self.assertIn("baml-bridge.$nuget.nupkg", cargo_tests)
        self.assertIn("left.ReadExactly(leftChunk)", normalizer)
        self.assertIn("right.ReadExactly(rightChunk)", normalizer)
        self.assertNotIn("left.Read(leftBuffer)", normalizer)


if __name__ == "__main__":
    unittest.main()
