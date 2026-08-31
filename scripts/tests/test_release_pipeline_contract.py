from __future__ import annotations

import copy
import hashlib
import json
import re
import subprocess
import tempfile
import unittest
from collections.abc import Callable
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CONTRACT_TOOL = ROOT / "scripts" / "baml-csharp-release-contract"
VERSION_TOOL = ROOT / "scripts" / "baml-language-version"
GO_RELEASE_SMOKE = ROOT / "scripts" / "smoke-go-release.py"
GO_SDK_ASSEMBLER = ROOT / "scripts" / "assemble-go-sdk-mirror"
PLATFORMS = ROOT / "release" / "platforms.json"
RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "release-baml-language.yml"
RELEASE_NOTIFIER = ROOT / "tools" / "notify-release-failure.py"
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yaml"
NIGHTLY_WORKFLOW = ROOT / ".github" / "workflows" / "nightly-release.yml"
BRIDGE_CFFI_PUBLIC_EXPORTS = (
    ROOT / "release" / "bridge-cffi-public-exports.txt"
)
HYGIENE_TOOL = ROOT / "scripts" / "baml-bridge-cffi-hygiene"
CROSS_CONFIG = ROOT / "baml_language" / "Cross.toml"
NUGET_PUBLISHER = ROOT / ".github" / "workflows" / "publish2-csharp-sdk.yaml"
NODE_NPM_PUBLISHER = (
    ROOT / ".github" / "workflows" / "publish2-nodejs-sdk.yaml"
)
NODE_BUILDER = (
    ROOT / ".github" / "workflows" / "build2-nodejs-sdk.reusable.yaml"
)
WEB_NPM_PUBLISHER = ROOT / ".github" / "workflows" / "publish2-web-sdk.yaml"
CSHARP_PREPARER = (
    ROOT / ".github" / "workflows" / "prepare-csharp-sdk.reusable.yaml"
)
CSHARP_VERIFIER = (
    ROOT
    / ".github"
    / "workflows"
    / "verify-csharp-product-slice.reusable.yaml"
)
CARGO_TESTS = ROOT / ".github" / "workflows" / "cargo-tests.reusable.yaml"
PACK_E2E = (
    ROOT / "baml_language" / "crates" / "baml_cli" / "tests" / "pack_e2e.rs"
)
CFFI_BUILDER = (
    ROOT / ".github" / "workflows" / "build2-bridge-cffi.reusable.yaml"
)
CPP_VERIFIER = (
    ROOT / ".github" / "workflows" / "verify-cpp-sdk.reusable.yaml"
)
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
NUGET_PACKAGE_SMOKE = (
    ROOT
    / "baml_language"
    / "sdks"
    / "csharp"
    / "bridge_csharp"
    / "tests"
    / "Baml.Bridge.NuGetPackageSmoke"
    / "verify.sh"
)
PKG_BOUNDARYML_COM_STACK = (
    ROOT / "tools" / "pkg_boundaryml_com" / "lib" / "pkg-boundaryml-com-stack.ts"
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

    def write_generated_sources(self, root: Path) -> Path:
        generated = root / "baml_sdk"
        sources = {
            "Baml/Generated/BamlProgram.g.cs": "program",
            "Baml/Http/Request.g.cs": "request",
            "CsharpBasicCalls/Functions.g.cs": "functions",
        }
        for relative, content in sources.items():
            path = generated / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        return generated

    def test_generated_source_manifest_accepts_growth_and_pins_exact_content(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            generated = self.write_generated_sources(root)
            manifest = root / "generated-sources.sha256"
            self.run_tool(
                "write-generated-manifest",
                "--root",
                str(generated),
                "--manifest",
                str(manifest),
            )
            self.run_tool(
                "verify-generated-sources",
                "--root",
                str(generated),
                "--manifest",
                str(manifest),
            )
            rows = manifest.read_text(encoding="utf-8").splitlines()
            self.assertEqual(
                [row.split("  ", maxsplit=1)[1] for row in rows],
                [
                    "Baml/Generated/BamlProgram.g.cs",
                    "Baml/Http/Request.g.cs",
                    "CsharpBasicCalls/Functions.g.cs",
                ],
            )

            (generated / "Baml/Http/Request.g.cs").write_text(
                "modified",
                encoding="utf-8",
            )
            result = self.run_tool(
                "verify-generated-sources",
                "--root",
                str(generated),
                "--manifest",
                str(manifest),
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("modified=['Baml/Http/Request.g.cs']", result.stderr)

    def test_generated_source_manifest_rejects_missing_and_unexpected_inputs(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            generated = self.write_generated_sources(root)
            manifest = root / "generated-sources.sha256"
            self.run_tool(
                "write-generated-manifest",
                "--root",
                str(generated),
                "--manifest",
                str(manifest),
            )
            extra = generated / "Baml/Time/Duration.g.cs"
            extra.parent.mkdir(parents=True, exist_ok=True)
            extra.write_text("duration", encoding="utf-8")
            result = self.run_tool(
                "verify-generated-sources",
                "--root",
                str(generated),
                "--manifest",
                str(manifest),
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unexpected=['Baml/Time/Duration.g.cs']", result.stderr)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            generated = self.write_generated_sources(root)
            (generated / "CsharpBasicCalls/Functions.g.cs").unlink()
            result = self.run_tool(
                "write-generated-manifest",
                "--root",
                str(generated),
                "--manifest",
                str(root / "generated-sources.sha256"),
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("missing required entry points", result.stderr)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            generated = self.write_generated_sources(root)
            (generated / "unexpected.json").write_text("{}", encoding="utf-8")
            result = self.run_tool(
                "write-generated-manifest",
                "--root",
                str(generated),
                "--manifest",
                str(root / "generated-sources.sha256"),
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("non-.g.cs file", result.stderr)

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
        plan = json.loads(self.write_plan(root).read_text(encoding="utf-8"))
        return self.run_tool(
            "stage-native",
            "--downloads",
            str(downloads),
            "--staging",
            str(root / "staging"),
            "--source-sha",
            "1" * 40,
            "--release-version",
            plan["canonical_version"],
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
        subprocess.run(
            [
                str(VERSION_TOOL),
                "plan",
                "--channel",
                "canary",
                "--out",
                str(path),
            ],
            cwd=ROOT,
            check=True,
            text=True,
            capture_output=True,
        )
        return path

    def verify(
        self,
        root: Path,
        *,
        plan: Path | None = None,
        check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        return self.run_tool(
            "verify-product",
            "--release-plan",
            str(plan or self.write_plan(root)),
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

    def test_version_tool_plan_is_accepted_and_schema_2_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.stage(root, self.make_downloads(root))
            plan_path = self.write_plan(root)
            plan = json.loads(plan_path.read_text(encoding="utf-8"))
            self.assertEqual(plan["schema"], 3)
            self.assertIn("nuget", plan["registry_versions"])
            self.verify(root, plan=plan_path)

            plan["schema"] = 2
            plan_path.write_text(json.dumps(plan), encoding="utf-8")
            result = self.verify(root, plan=plan_path, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "release plan must be a schema-3 JSON object",
                result.stderr,
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
            with self.subTest(option=option), tempfile.TemporaryDirectory() as temporary:
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


class BridgeCffiHygieneTests(unittest.TestCase):
    def verify_payload(self, payload: bytes) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as temporary:
            native = Path(temporary) / "native.bin"
            native.write_bytes(payload)
            return subprocess.run(
                [
                    str(HYGIENE_TOOL),
                    "verify",
                    "--native",
                    str(native),
                    "--target",
                    "test-target",
                ],
                cwd=ROOT,
                check=False,
                text=True,
                capture_output=True,
            )

    def test_rejects_ascii_cross_and_runner_paths(self) -> None:
        payloads = (
            b"prefix\0/cargo/registry/src/aws-lc/source.c\0suffix",
            b"/home/runner/work/baml/baml/source.rs",
            b"/root/.cargo/registry/src/dependency.c",
        )
        for payload in payloads:
            with self.subTest(payload=payload):
                result = self.verify_payload(payload)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("absolute build-tree paths", result.stderr)

    def test_rejects_utf16le_windows_paths(self) -> None:
        secret_path = (
            "C:\\Users\\runneradmin\\.cargo\\registry\\src\\dependency.c"
        ).encode("utf-16le")
        result = self.verify_payload(secret_path)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("C:\\Users\\runneradmin\\.cargo", result.stderr)

    def test_rejects_quoted_cross_paths(self) -> None:
        payloads = (
            b'cargo "/project/aws-lc/source.c"',
            b"cargo '/cargo/registry/src/dependency.c'",
            b'rustc "/rust/toolchains/stable/lib/libstd.rlib"',
            b"rustc '/rust/settings.toml'",
        )
        for payload in payloads:
            with self.subTest(payload=payload):
                result = self.verify_payload(payload)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("absolute build-tree paths", result.stderr)

    def test_allows_stable_mapped_and_rust_dependency_paths(self) -> None:
        payload = b"\0".join(
            (
                b"cargo-home/registry/src/dependency.c",
                b"baml-source/crates/bridge_cffi/src/lib.rs",
                b"rust-toolchain/lib/rustlib/src/rust/library/std/src/lib.rs",
                b"/rust/deps/compiler_builtins/src/lib.rs",
                b"http://metadata.google.internal/computeMetadata/v1/project/project-id",
            )
        )
        result = self.verify_payload(payload)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_credentials_without_echoing_them(self) -> None:
        credential = "ghp_" + ("A" * 30)
        result = self.verify_payload(credential.encode())
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("GitHub token", result.stderr)
        self.assertNotIn(credential, result.stderr)

    def test_bounds_path_diagnostics(self) -> None:
        payload = b"\0".join(
            f"/cargo/registry/src/dependency-{index}/source.c".encode()
            for index in range(12)
        )
        result = self.verify_payload(payload)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stderr.count("  - /cargo/"), 8)
        self.assertIn("showing 8 of 12 unique paths", result.stderr)


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


def step_block(job: str, step: str) -> str:
    marker = f"      - name: {step}\n"
    start = job.index(marker)
    remainder = job[start + len(marker) :]
    lines = remainder.splitlines(keepends=True)
    next_step = next(
        (
            index
            for index, line in enumerate(lines)
            if line.startswith("      - ")
        ),
        None,
    )
    if next_step is None:
        return remainder
    return "".join(lines[:next_step])


def step_inputs(step: str) -> dict[str, str]:
    marker = "        with:\n"
    start = step.index(marker)
    inputs: dict[str, str] = {}
    for line in step[start + len(marker) :].splitlines():
        if not line.startswith("          "):
            break
        key, value = line.strip().split(":", maxsplit=1)
        inputs[key] = value.strip().strip("\"'")
    return inputs


class WorkflowGraphTests(unittest.TestCase):
    def test_release_slack_notifications_cover_failures_and_canary_success(self) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        notifier = RELEASE_NOTIFIER.read_text(encoding="utf-8")
        notify_slack = job_block(workflow, "notify-slack")
        notify_step = step_block(notify_slack, "Notify Slack of the release result")

        self.assertIn("always()", notify_slack)
        self.assertIn(
            "run: uv run --script tools/notify-release-failure.py",
            notify_step,
        )
        self.assertIn(
            "CHANNEL: ${{ needs.plan.outputs.channel }}",
            notify_step,
        )
        self.assertIn(
            "VERSION: ${{ needs.plan.outputs.version }}",
            notify_step,
        )
        self.assertIn(
            "RELEASE_SUCCEEDED: ${{ needs.publish-pkg-channel.result == 'success' }}",
            notify_step,
        )
        self.assertIn(
            'if release_succeeded and not failures and channel != "canary"',
            notifier,
        )
        self.assertIn(
            'SUCCESSFUL_JOB_CONCLUSIONS = {"success", "skipped"}',
            notifier,
        )
        self.assertIn("conclusion not in SUCCESSFUL_JOB_CONCLUSIONS", notifier)
        self.assertIn("❌ BAML {channel} release failed", notifier)
        self.assertIn("✅ BAML {channel} release succeeded", notifier)

    def test_cpp_verifier_stamps_the_frozen_release_plan(self) -> None:
        workflow = CPP_VERIFIER.read_text(encoding="utf-8")
        verify = job_block(workflow, "verify")
        stamp = step_block(verify, "Stamp release plan")

        self.assertIn(
            "scripts/baml-language-version stamp --plan release-plan.json",
            stamp,
        )
        self.assertLess(
            stamp.index("printf '%s\\n' \"$RELEASE_PLAN_JSON\""),
            stamp.index("scripts/baml-language-version stamp"),
        )
        self.assertLess(
            workflow.index("scripts/baml-language-version stamp"),
            workflow.index("cmake -S baml_language/sdks/cpp/bridge_cpp/tests"),
        )

    def test_node_musl_abi_check_uses_runtime_dependencies(self) -> None:
        workflow = NODE_BUILDER.read_text(encoding="utf-8")
        verify = step_block(workflow, "Verify musl native addon ABI")

        self.assertIn('readelf --wide --dynamic "$native"', verify)
        self.assertIn('needed="$(grep NEEDED <<<"$dynamic")"', verify)
        self.assertIn("'\\[libc\\.so\\]'", verify)
        self.assertIn("'libc\\.so\\.6'", verify)
        self.assertNotIn("readelf --version-info", verify)
        self.assertNotIn("grep -q 'GLIBC_'", verify)

    def test_cargo_jobs_name_targets_and_run_pack_e2e_on_musl(self) -> None:
        workflow = CARGO_TESTS.read_text(encoding="utf-8")
        for target in (
            "x86_64-unknown-linux-gnu",
            "x86_64-unknown-linux-musl",
            "aarch64-apple-darwin",
            "x86_64-pc-windows-msvc",
            "wasm32-unknown-unknown",
        ):
            self.assertIn(f'name: "cargo test ({target})"', workflow)

        musl = job_block(workflow, "cargo-test-linux-musl")
        self.assertIn("CARGO_BUILD_TARGET: x86_64-unknown-linux-musl", musl)
        self.assertIn("uses: ./.github/actions/setup-musl-cross", musl)
        self.assertIn("cargo nextest run --workspace", musl)
        self.assertIn("cargo test -p baml_cli --test pack_e2e", musl)
        self.assertNotIn("--all-features", musl)
        self.assertEqual(workflow.count("- cargo-test-linux-musl"), 2)

        pack_e2e = PACK_E2E.read_text(encoding="utf-8")
        self.assertIn("function main() -> never", pack_e2e)
        self.assertIn("baml.sys.exit(42)", pack_e2e)
        self.assertIn("Some(42)", pack_e2e)

    def test_cffi_hygiene_is_enforced_at_production_and_package_boundaries(
        self,
    ) -> None:
        builder = CFFI_BUILDER.read_text(encoding="utf-8")
        cross = CROSS_CONFIG.read_text(encoding="utf-8")
        pack = PACK_PRODUCT.read_text(encoding="utf-8")
        ci = CI_WORKFLOW.read_text(encoding="utf-8")
        ci_builder = job_block(ci, "bridge-cffi-release")
        ci_alert = job_block(ci, "ci-failure-alert")
        ci_dispatch = job_block(ci, "dispatch-release")

        self.assertIn("-ffile-prefix-map=/cargo=cargo-home", builder)
        self.assertIn("-fdebug-prefix-map=/cargo=cargo-home", builder)
        self.assertIn('-D__FILE__=\\"baml-source/native\\"', builder)
        self.assertIn("cygpath -aw", builder)
        self.assertIn("--short-name", builder)
        self.assertIn(
            "/clang:-ffile-prefix-map=$cargo_root_native=cargo-home", builder
        )
        self.assertIn(
            "/clang:-ffile-prefix-map=$cargo_root_native_short=cargo-home", builder
        )
        self.assertIn('export "CC_${target_key}=clang-cl"', builder)
        self.assertIn('export "CXX_${target_key}=clang-cl"', builder)
        self.assertIn("AWS_LC_SYS_NO_JITTER_ENTROPY=1", builder)
        self.assertIn("dumpbin /nologo /exports", builder)
        self.assertIn("release\\bridge-cffi-public-exports.txt", builder)
        self.assertEqual(builder.count("Sort-Object -CaseSensitive -Unique"), 2)
        self.assertIn("Compare-Object -CaseSensitive", builder)
        public_exports = BRIDGE_CFFI_PUBLIC_EXPORTS.read_text(
            encoding="utf-8"
        ).splitlines()
        self.assertEqual(public_exports, sorted(set(public_exports)))
        self.assertIn("baml_get_api_v1", public_exports)
        self.assertFalse(
            any(export.startswith("aws_lc_") for export in public_exports)
        )
        self.assertLess(
            builder.index("dumpbin /nologo /exports"),
            builder.index("- name: Upload cdylib artifact"),
        )
        self.assertIn('export CFLAGS="${CFLAGS:+$CFLAGS }$native_path_maps"', builder)
        self.assertIn(
            'export CXXFLAGS="${CXXFLAGS:+$CXXFLAGS }$native_path_maps"',
            builder,
        )
        self.assertLess(
            builder.index("- name: Verify shipping artifact hygiene"),
            builder.index("- name: Upload cdylib artifact"),
        )
        self.assertEqual(
            cross.count('passthrough = ["RUSTFLAGS", "CFLAGS", "CXXFLAGS"]'),
            4,
        )
        self.assertIn("scripts/baml-bridge-cffi-hygiene", pack)
        self.assertIn(
            "release/bridge-cffi-public-exports.txt",
            pack,
        )
        self.assertNotIn('strings "$native"', pack)

        self.assertIn("bridge_cffi_release:", ci)
        self.assertIn(
            "uses: ./.github/workflows/build2-bridge-cffi.reusable.yaml",
            ci_builder,
        )
        self.assertIn("source_sha: ${{ github.sha }}", ci_builder)
        self.assertIn("- bridge-cffi-release", ci_alert)
        self.assertIn("- bridge-cffi-release", ci_dispatch)
        self.assertIn(
            '-f source_ci_run_id="$GITHUB_RUN_ID"',
            ci_dispatch,
        )
        self.assertIn("contents: write", ci_dispatch)
        self.assertIn(
            'release_ref="baml-language-source-$GITHUB_SHA"',
            ci_dispatch,
        )
        self.assertIn(
            '-f ref="refs/tags/$release_ref"',
            ci_dispatch,
        )
        self.assertIn(
            'ref_type" != "commit"',
            ci_dispatch,
        )
        self.assertIn(
            'ref_sha" != "$GITHUB_SHA"',
            ci_dispatch,
        )
        self.assertIn(
            '--ref "$RELEASE_WORKFLOW_REF"',
            ci_dispatch,
        )
        self.assertNotIn("--ref canary", ci_dispatch)

    def test_production_release_is_ci_attested_and_least_privilege(self) -> None:
        release = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        ci = CI_WORKFLOW.read_text(encoding="utf-8")
        pkg_stack = PKG_BOUNDARYML_COM_STACK.read_text(encoding="utf-8")
        plan = job_block(release, "plan")
        prerequisites = job_block(release, "release-prerequisites-complete")
        complete = job_block(release, "release-complete")
        manifest = job_block(release, "publish-pkg-boundaryml-com")
        channel = job_block(release, "publish-pkg-channel")
        dry_run = job_block(release, "dry-run-artifacts")
        nightly = NIGHTLY_WORKFLOW.read_text(encoding="utf-8")

        self.assertIn("source_ci_run_id:", release)
        self.assertIn("Attest production source CI run", plan)
        self.assertIn("production releases require a numeric source_ci_run_id", plan)
        self.assertIn("CI - BAML Language", plan)
        self.assertIn('"$workflow_path" != ".github/workflows/ci.yaml"', plan)
        self.assertIn('"$event" != "push"', plan)
        self.assertIn('"$branch" != "canary"', plan)
        self.assertIn('"$head_sha" != "$INPUT_SOURCE_SHA"', plan)
        self.assertIn('"$conclusion" != "success"', plan)
        self.assertIn('WORKFLOW_SOURCE_SHA: ${{ github.sha }}', plan)
        self.assertIn('WORKFLOW_REF_NAME: ${{ github.ref_name }}', plan)
        self.assertIn('WORKFLOW_REF_TYPE: ${{ github.ref_type }}', plan)
        self.assertIn(
            '"$WORKFLOW_SOURCE_SHA" != "$INPUT_SOURCE_SHA"',
            plan,
        )
        self.assertIn(
            '"$WORKFLOW_REF_TYPE" != "tag"',
            plan,
        )
        self.assertIn(
            '"$WORKFLOW_REF_NAME" != "$expected_workflow_ref"',
            plan,
        )
        self.assertIn('if ! run_json="$(gh api', plan)
        self.assertIn("gh api call failed (attempt $attempt/30); retrying", plan)
        self.assertIn(
            "permissions:\n  contents: read\n  actions: read\n",
            release,
        )
        for block in (manifest, channel, dry_run):
            self.assertIn("id-token: write", block)
        self.assertIn(
            "needs.all-builds.result == 'success'",
            prerequisites,
        )
        self.assertIn(
            "needs.release-prerequisites-complete.result == 'success'",
            complete,
        )
        self.assertIn(
            '-f source_ci_run_id="$GITHUB_RUN_ID"',
            job_block(ci, "dispatch-release"),
        )
        # The nightly scheduler is the other production dispatcher, and is held
        # to the same trust chain: attested CI run, dispatched at the source tag.
        self.assertIn(
            '-f source_ci_run_id="$SOURCE_CI_RUN_ID"',
            nightly,
        )
        self.assertIn(
            'release_ref="baml-language-source-$SOURCE_SHA"',
            nightly,
        )
        self.assertIn(
            '--ref "$release_ref"',
            nightly,
        )
        self.assertNotIn("--ref canary", nightly)
        self.assertIn(
            "`repo:${GITHUB_REPO}:ref:refs/heads/canary`",
            pkg_stack,
        )
        self.assertIn(
            "`repo:${GITHUB_REPO}:ref:refs/tags/baml-language-source-*`",
            pkg_stack,
        )
        self.assertIn(
            "'token.actions.githubusercontent.com:sub': GITHUB_OIDC_SUBJECTS",
            pkg_stack,
        )
        self.assertNotIn(
            "`repo:${GITHUB_REPO}:ref:*`",
            pkg_stack,
        )

    def test_go_release_uses_both_frozen_version_identities(self) -> None:
        release = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        assembler = GO_SDK_ASSEMBLER.read_text(encoding="utf-8")
        build = job_block(release, "build-go-sdk")
        publish = job_block(release, "publish-go-sdk")

        self.assertIn("go_version: ${{ steps.plan.outputs.go_version }}", release)
        self.assertIn('GO_VERSION: ${{ needs.plan.outputs.go_version }}', build)
        self.assertIn('--go-version "$GO_VERSION"', build)
        self.assertIn('GO_VERSION: ${{ needs.plan.outputs.go_version }}', publish)
        self.assertIn('tag="$GO_VERSION"', publish)
        self.assertIn('"version": $go_version', publish)
        self.assertNotIn('tag="v$CANONICAL_VERSION"', publish)
        self.assertIn('parser.add_argument("--go-version", required=True)', assembler)
        self.assertIn("runtime_matches != [args.go_version]", assembler)

    def test_nightly_is_scheduled_and_pushes_only_cut_canary(self) -> None:
        release = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        ci_dispatch = job_block(CI_WORKFLOW.read_text(encoding="utf-8"), "dispatch-release")
        nightly_workflow = NIGHTLY_WORKFLOW.read_text(encoding="utf-8")
        dispatch_job = job_block(nightly_workflow, "dispatch-nightly")
        toolchain = job_block(release, "publish-toolchain-release")
        cffi = job_block(release, "publish-bridge-cffi-release")

        # A push to canary cuts the canary channel only, and only when
        # release.toml asks for it. Nothing else releases on merge.
        self.assertIn("-f channel=canary", ci_dispatch)
        self.assertNotIn("-f channel=nightly", ci_dispatch)
        # Bound the slice to the channel input itself rather than a character
        # count, so it cannot drift into a neighbouring input's description.
        # `auto` used to let CI defer the channel choice to the release.
        channel_input = release[
            release.index("      channel:") : release.index("      dry_run:")
        ]
        self.assertIn("options: [nightly, canary]", channel_input)
        self.assertNotIn("auto", channel_input)
        self.assertIn(
            "git diff --quiet HEAD^ HEAD -- baml_language/release.toml",
            ci_dispatch,
        )
        # The file changing is only the fast path; the cut turns on the parsed
        # [release].canary_version differing, so that an edit to anything else
        # in release.toml cannot dispatch a release.
        self.assertIn('git show "HEAD^:baml_language/release.toml"', ci_dispatch)
        self.assertIn(
            'if [[ "$canary_version" == "$previous_version" ]]', ci_dispatch
        )
        self.assertIn("steps.canary-cut.outputs.cut == 'true'", ci_dispatch)
        # The source tag is minted for EVERY green canary push, not just the
        # cuts: the scheduled nightly dispatches at a tag CI may have created
        # days earlier, and the release refuses any other ref.
        tag_step = step_block(ci_dispatch, "Create immutable release workflow ref")
        self.assertIn('release_ref="baml-language-source-$GITHUB_SHA"', tag_step)
        self.assertNotIn("if:", tag_step)

        # Two crons bracket both DST offsets; the gate keeps whichever one is
        # local midnight. Both halves matter: dropping the gate double-cuts, and
        # inverting it cuts at the wrong hour.
        crons = re.findall(r"- cron: ['\"](\S+) (\S+) [^'\"]*['\"]", nightly_workflow)
        self.assertEqual(len(crons), 2, nightly_workflow)
        self.assertEqual({hour for _, hour in crons}, {"7", "8"}, crons)
        for minute, _ in crons:
            # Not on the hour: GitHub delays scheduled runs most at :00, and a
            # delay past 01:00 local forfeits the night.
            self.assertNotEqual(minute, "0", crons)
        self.assertIn('local_hour="$(TZ=America/Los_Angeles date +%H)"', nightly_workflow)
        # Assert the gate's POLARITY, not just that both words appear: the
        # midnight branch proceeds and the off-hour branch stands down. Swapping
        # the two would cut at 23:00 or 01:00 and still contain both strings.
        gate = re.search(
            r'if \[\[ "\$local_hour" == "00" \]\]; then(.*?)\belse\b(.*?)\bfi\b',
            step_block(dispatch_job, "Gate on local midnight in Seattle"),
            flags=re.DOTALL,
        )
        self.assertIsNotNone(gate, dispatch_job)
        at_midnight, off_hour = gate.groups()
        self.assertIn("proceed=true", at_midnight)
        self.assertNotIn("proceed=false", at_midnight)
        self.assertIn("proceed=false", off_hour)
        self.assertNotIn("proceed=true", off_hour)

        # One cut per commit, and the guard must observe the EFFECT -- a release
        # run at the candidate's source tag -- rather than this workflow's own
        # conclusion. The clock gate skips steps, not the job, so the cron entry
        # that loses the gate concludes `success` too; a self-referential guard
        # would read that no-op as "tonight is handled" and stand the real cut
        # down every night for the whole PST half of the year.
        self.assertIn("gh run list --workflow release-baml-language.yml", dispatch_job)
        self.assertIn("baml-language-source-$CANDIDATE_SHA", dispatch_job)
        self.assertIn('if [[ "$existing" != "0"', dispatch_job)
        self.assertNotIn("--workflow nightly-release.yml", dispatch_job)

        # A fork inherits the schedule and must never cut releases upstream.
        self.assertIn("github.repository == 'BoundaryML/baml'", dispatch_job)

        # Only a CI-attested canary commit is releasable, dispatched at its own
        # immutable tag with the run that attests it.
        self.assertIn(
            "gh run list --workflow ci.yaml --branch canary --event push",
            nightly_workflow,
        )
        self.assertIn("commits?sha=canary", nightly_workflow)
        self.assertIn('release_ref="baml-language-source-$SOURCE_SHA"', nightly_workflow)
        self.assertIn('--ref "$release_ref"', nightly_workflow)
        self.assertNotIn("--ref canary", nightly_workflow)
        self.assertIn("-f channel=nightly", nightly_workflow)
        self.assertIn('-f source_ci_run_id="$SOURCE_CI_RUN_ID"', nightly_workflow)
        # The dispatch verifies the tag resolves to exactly the attested commit.
        self.assertIn('"$ref_sha" != "$SOURCE_SHA"', nightly_workflow)

        # The schedule is the ONLY nightly cut site. A canary release used to
        # chain one, which meant two sites racing to name the same night.
        self.assertNotIn("dispatch-nightly-after-canary", release)
        self.assertNotIn("dispatch_nightly_after_canary", release)
        self.assertNotIn("nightly-slot", release + nightly_workflow)
        self.assertNotIn("nightly_date", release + nightly_workflow)

        # `force` must not reach the rollback guard: re-running with it is the
        # obvious reaction to that failure, and would publish the rollback.
        rollback_arm = re.search(
            r"behind\s*\|\s*diverged\)(.*?);;", nightly_workflow, flags=re.DOTALL
        )
        self.assertIsNotNone(rollback_arm, nightly_workflow)
        self.assertNotIn("FORCE", rollback_arm.group(1))

        # The release tag must name the commit the artifacts were built from:
        # the scheduler reads it to decide whether canary moved.
        self.assertIn('--target "$SOURCE_SHA"', toolchain)
        self.assertIn('--target "$SOURCE_SHA"', cffi)

    def test_release_graph_has_early_preflight_parallel_producers_and_complete_fanin(
        self,
    ) -> None:
        """Keep the release graph complete and its wrapper compatibility gates enforced."""
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        build_matrix = job_block(workflow, "build-matrix")
        wrapper = job_block(workflow, "build-wrapper")
        prepare = job_block(workflow, "prepare-csharp-sdk")
        cffi = job_block(workflow, "build-bridge-cffi")
        verify = job_block(workflow, "verify-csharp-sdk")
        all_builds = job_block(workflow, "all-builds")
        nuget = job_block(workflow, "publish-csharp-sdk")
        crates_io = job_block(workflow, "publish-crates-io")
        wrapper_release = job_block(workflow, "publish-wrapper-release")
        prerequisites = job_block(workflow, "release-prerequisites-complete")
        notify_slack = job_block(workflow, "notify-slack")
        complete = job_block(workflow, "release-complete")
        manifest = job_block(workflow, "publish-pkg-boundaryml-com")
        go_publisher = job_block(workflow, "publish-go-sdk")
        channel = job_block(workflow, "publish-pkg-channel")
        dry_run = job_block(workflow, "dry-run-artifacts")

        self.assertIn("baml-csharp-release-contract matrix", build_matrix)
        self.assertIn("baml-release-platforms wrapper-matrix", build_matrix)
        self.assertIn("needs.build-matrix.outputs.wrapper", wrapper)
        self.assertIn("matrix.no_self_update", wrapper)
        self.assertIn("matrix.archive_suffix", wrapper)
        self.assertIn("matrix.executable_suffix", wrapper)
        self.assertIn("Install cross for backward-compatible GNU wrapper", wrapper)
        self.assertIn("cargo install cross --locked --version 0.2.5", wrapper)
        self.assertIn("Verify Linux wrapper ABI and installer selection", wrapper)
        self.assertIn("readelf --wide --dynamic", wrapper)
        self.assertIn("GLIBC_[0-9]+", wrapper)
        self.assertIn('matrix.glibc_max', wrapper)
        self.assertIn("--network none", wrapper)
        self.assertIn("sh /repo/scripts/install.sh --wrapper-only", wrapper)
        self.assertIn(
            "debian@sha256:abd67ffcfa541b485a3dff59865ab629aa048a6c613e639d36e7456b0b229241",
            wrapper,
        )
        self.assertIn(
            "alpine@sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce",
            wrapper,
        )
        self.assertIn("verify-wrapper-artifacts", wrapper_release)
        self.assertNotIn("unknown-linux-gnu' ||", wrapper)
        self.assertNotIn("windows-msvc", wrapper)
        self.assertIn("--features no-self-update", wrapper)
        self.assertIn("needs: [plan, build-matrix]", prepare)
        self.assertNotIn("build-bridge-cffi", prepare)
        self.assertIn("needs: [plan]", cffi)
        self.assertIn("prepare-csharp-sdk", verify)
        self.assertIn("build-bridge-cffi", verify)
        self.assertIn("- verify-csharp-sdk", all_builds)
        self.assertIn("all-builds", nuget)
        self.assertIn("environment: boundary-tools-prod", crates_io)
        self.assertNotIn("environment: release", crates_io)
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
        ):
            self.assertIn(f"- {publisher}", prerequisites)
        # AUR is temporarily excluded while upstream maintenance rejects SSH clones.
        self.assertNotIn("\n  publish-aur:\n", workflow)
        self.assertNotRegex(prerequisites, r"(?m)^\s+- publish-aur\s*$")
        self.assertNotRegex(notify_slack, r"(?m)^\s+- publish-aur\s*$")
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
                ("wrapper_changed", "WRAPPER_CHANGED"),
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

        swift_verify = job_block(workflow, "verify-swift-sdk")
        swift_smoke = job_block(workflow, "smoke-swift-release")
        self.assertIn("BamlRuntime.nativeVersion()", swift_verify)
        self.assertIn("BamlRuntime.nativeVersion()", swift_smoke)
        self.assertIn(
            '--cfg=getrandom_backend=\\"wasm_js\\"',
            CFFI_BUILDER.read_text(encoding="utf-8"),
        )

    def test_expected_nightly_skips_do_not_poison_release_tail(self) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        prerequisites = job_block(workflow, "release-prerequisites-complete")
        manifest = job_block(workflow, "publish-pkg-boundaryml-com")
        go_publisher = job_block(workflow, "publish-go-sdk")
        go_smoke = job_block(workflow, "smoke-go-release")
        channel = job_block(workflow, "publish-pkg-channel")

        self.assertIn(
            'require_skipped Gradle-Plugin-Portal "$GRADLE_PLUGIN"',
            prerequisites,
        )
        guarded_tail = (
            (
                manifest,
                (
                    "needs.plan.result == 'success'",
                    "needs.release-prerequisites-complete.result == 'success'",
                ),
            ),
            (
                go_publisher,
                (
                    "needs.plan.result == 'success'",
                    "needs.build-go-sdk.result == 'success'",
                    "needs.all-builds.result == 'success'",
                    "needs.publish-pkg-boundaryml-com.result == 'success'",
                ),
            ),
            (
                go_smoke,
                (
                    "needs.plan.result == 'success'",
                    "needs.publish-go-sdk.result == 'success'",
                    "needs.publish-pkg-boundaryml-com.result == 'success'",
                ),
            ),
            (
                channel,
                (
                    "needs.plan.result == 'success'",
                    "needs.publish-pkg-boundaryml-com.result == 'success'",
                    "needs.release-complete.result == 'success'",
                    "needs.smoke-go-release.result == 'success'",
                    "needs.smoke-swift-release.result == 'success'",
                ),
            ),
        )
        for block, required_results in guarded_tail:
            self.assertIn("!cancelled()", block)
            for result in required_results:
                self.assertIn(result, block)

    def test_go_release_smoke_reconciles_complete_dependency_graph(self) -> None:
        smoke = GO_RELEASE_SMOKE.read_text(encoding="utf-8")

        self.assertIn('["go", "mod", "tidy"]', smoke)
        self.assertNotIn(
            '["go", "mod", "download",',
            smoke,
        )

    def test_nuget_repair_and_nightly_pack_contracts_are_fail_closed(self) -> None:
        publisher = NUGET_PUBLISHER.read_text(encoding="utf-8")
        preparer = CSHARP_PREPARER.read_text(encoding="utf-8")
        verifier = CSHARP_VERIFIER.read_text(encoding="utf-8")
        nuget_package_smoke = NUGET_PACKAGE_SMOKE.read_text(encoding="utf-8")
        release = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        cargo_tests = CARGO_TESTS.read_text(encoding="utf-8")
        pack = PACK_PRODUCT.read_text(encoding="utf-8")
        normalizer = NUGET_NORMALIZER.read_text(encoding="utf-8")
        self.assertIn('404)', publisher)
        self.assertIn('echo "publish=true"', publisher)
        self.assertIn('200)', publisher)
        self.assertIn('compare "$PACKAGE"', publisher)
        self.assertIn(
            "if: ${{ steps.existing.outputs.publish == 'true' }}",
            publisher,
        )
        smoke = publisher.index(
            "- name: Restore and execute the exact public version from a clean cache"
        )
        self.assertNotIn("\n        if:", publisher[smoke : smoke + 300])
        self.assertIn("environment: boundary-tools-prod", publisher)
        self.assertIn("publish2-csharp-sdk.yaml", publisher)
        self.assertNotIn("\n    secrets:\n", publisher)
        self.assertIn("user: ${{ vars.NUGET_USER }}", publisher)
        self.assertNotIn("secrets.NUGET_USER", publisher)
        self.assertNotIn("NUGET_USER: ${{ secrets.NUGET_USER }}", release)
        self.assertIn("write-generated-manifest", preparer)
        self.assertIn("verify-generated-sources", verifier)
        self.assertIn("generated-sources.sha256", verifier)
        self.assertIn("verify-generated-sources", publisher)
        self.assertNotIn("Baml/Csv/CsvError.g.cs", preparer)
        self.assertNotIn("Baml/Csv/CsvError.g.cs", verifier)
        self.assertNotIn("Baml/Csv/CsvError.g.cs", nuget_package_smoke)
        self.assertIn("list_generated_sources", nuget_package_smoke)
        self.assertNotIn("-printf", nuget_package_smoke)

        self.assertIn(".registry_versions.nuget", pack)
        self.assertIn('-p:PackageVersion="$nuget_version"', pack)
        self.assertIn('-p:InformationalVersion="$canonical_version"', pack)
        self.assertNotIn("baml-language-version\" show", pack)
        self.assertNotIn("workflow_run_attempt", pack)
        self.assertNotIn("baml-language-version show", verifier)
        self.assertIn(".registry_versions.nuget", verifier)
        self.assertIn('name: "Test nightly C# package version"', cargo_tests)
        self.assertIn('-p:PackageVersion="$nuget"', cargo_tests)
        self.assertIn("baml-bridge.$nuget.nupkg", cargo_tests)
        self.assertIn("left.ReadExactly(leftChunk)", normalizer)
        self.assertIn("right.ReadExactly(rightChunk)", normalizer)
        self.assertNotIn("left.Read(leftBuffer)", normalizer)

    def test_npm_publishers_do_not_enable_an_unused_pnpm_cache(self) -> None:
        for path in (NODE_NPM_PUBLISHER, WEB_NPM_PUBLISHER):
            with self.subTest(workflow=path.name):
                publisher = path.read_text(encoding="utf-8")
                publish_job = job_block(publisher, "publish-npm")
                setup = step_block(publish_job, "Setup Node.js")
                inputs = step_inputs(setup)
                self.assertIn("        uses: actions/setup-node@v6", setup)
                self.assertNotIn("./.github/actions/setup-node", setup)
                self.assertEqual(
                    inputs["registry-url"],
                    "https://registry.npmjs.org",
                )
                self.assertNotEqual(inputs.get("cache"), "pnpm")

    def test_csharp_consumer_repository_path_scan_requires_a_separator(self) -> None:
        nuget_package_smoke = NUGET_PACKAGE_SMOKE.read_text(encoding="utf-8")
        self.assertIn(
            'repository_path_prefix="${repository_root%/}/"',
            nuget_package_smoke,
        )
        self.assertIn(
            'grep -r -a -F -l -- "$repository_path_prefix" "$publish"',
            nuget_package_smoke,
        )
        self.assertNotIn(
            'grep -r -a -F -l -- "$repository_root" "$publish"',
            nuget_package_smoke,
        )
        self.assertNotIn("rg -a", nuget_package_smoke)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            benign_publish = root / "benign"
            leaked_publish = root / "leaked"
            benign_publish.mkdir()
            leaked_publish.mkdir()
            benign = benign_publish / "benign.bin"
            leaked = leaked_publish / "leaked.bin"
            benign.write_bytes(
                b"cargo-home/registry/src/tokio/src/runtime/metrics/worker.rs"
            )
            leaked.write_bytes(b"debug source: /work/baml/src/runtime.rs")

            benign_result = subprocess.run(
                [
                    str(NUGET_PACKAGE_SMOKE),
                    "--verify-repository-paths",
                    "/work",
                    str(benign_publish),
                ],
                check=False,
                text=True,
                capture_output=True,
            )
            self.assertEqual(benign_result.returncode, 0, benign_result.stderr)
            self.assertIn(b"/worker.rs", benign.read_bytes())

            leaked_result = subprocess.run(
                [
                    str(NUGET_PACKAGE_SMOKE),
                    "--verify-repository-paths",
                    "/work",
                    str(leaked_publish),
                ],
                check=False,
                text=True,
                capture_output=True,
            )
            self.assertEqual(leaked_result.returncode, 1)
            self.assertIn(
                "published consumer contains a repository path",
                leaked_result.stderr,
            )


if __name__ == "__main__":
    unittest.main()
