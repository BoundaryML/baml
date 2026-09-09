from contextlib import redirect_stdout
import io
import json
import os
import subprocess
from pathlib import Path
import tempfile
import unittest

import yaml

from rules import HERE, Linter, MISE, R2, RUST, main


class RuleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def write(self, path, data):
        p = self.root / path
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(data if isinstance(data, str) else yaml.safe_dump(data, sort_keys=False))
        return p

    def workflow(self, steps, path=".github/workflows/future.yml", **job):
        self.write(
            path,
            {
                "name": "Future",
                "env": {
                    key: "${{ secrets." + key + " }}"
                    for key in [
                        "BAML_SCCACHE_R2_ACCESS_KEY_ID",
                        "BAML_SCCACHE_R2_SECRET_ACCESS_KEY",
                    ]
                },
                "on": {"push": {}},
                "jobs": {"test": {"runs-on": "ubuntu-latest", "steps": steps, **job}},
            },
        )

    def action(self, steps, path="tools/future/action.yaml"):
        self.write(
            path,
            {
                "name": "Future",
                "description": "test",
                "runs": {"using": "composite", "steps": steps},
            },
        )

    def lint(self, exclusions=None):
        return Linter(self.root, exclusions or {}).run()

    def codes(self, exclusions=None):
        return [d.rule for d in self.lint(exclusions)]

    def canonical(self):
        self.action([{"uses": "jdx/mise-action@v4"}], MISE)
        self.action(
            [
                {
                    "uses": "./.github/actions/setup-mise",
                    "with": {"install_args": "sccache direnv"},
                },
                {"run": 'direnv allow .envrc\ndirenv export gha >> "$GITHUB_ENV"'},
            ],
            R2,
        )

    def test_bans_install_actions_at_any_version_and_case(self):
        for action in [
            "Swatinem/rust-cache",
            "actions/setup-node",
            "actions/setup-python",
            "actions/setup-go",
            "actions/setup-dotnet",
            "pnpm/action-setup",
            "astral-sh/setup-uv",
            "jdx/mise-action",
            "dtolnay/rust-toolchain",
            "taiki-e/install-action",
            "mozilla-actions/sccache-action",
            "useblacksmith/rust-cache",
            "ruby/setup-ruby",
            "gradle/actions/setup-gradle",
            "mlugg/setup-zig",
            "cargo-bins/cargo-binstall",
            "new-owner/innocent-name",
        ]:
            with self.subTest(action=action):
                self.workflow([{"uses": action.upper() + "@" + "a" * 40}])
                self.assertIn("tool-action", self.codes())

    def test_baml_setup_is_explicitly_approved(self):
        self.workflow([{"uses": "BoundaryML/setup-baml@v1"}])
        self.assertEqual([], self.lint())

    def test_setup_mise_bootstrap_is_not_a_file_exclusion(self):
        self.action(
            [{"uses": "jdx/mise-action@v4"}, {"uses": "cargo-bins/cargo-binstall@main"}], MISE
        )
        self.assertEqual([], self.lint())
        self.action([{"uses": "jdx/mise-action@v4"}, {"uses": "actions/setup-node@v7"}], MISE)
        self.assertEqual(["tool-action"], self.codes())
        self.action([{"run": "cargo install cross"}], MISE)
        self.assertEqual(["tool-install"], self.codes())

    def test_bootstrap_does_not_apply_to_similarly_named_action(self):
        self.action([{"uses": "jdx/mise-action@v4"}], ".github/actions/setup-mise-copy/action.yaml")
        self.assertEqual(["tool-action"], self.codes())

    def test_windows_rustup_bootstrap_selects_runner_architecture(self):
        action = yaml.safe_load((HERE.parents[1] / RUST).read_text())
        script = action["runs"]["steps"][0]["run"]
        # Execute the real bootstrap with rustup absent and downloads stubbed.
        prefix = """
command() { if [[ "$*" == "-v rustup" ]]; then return 1; else builtin command "$@"; fi; }
curl() { printf '%s\\n' "$*" >> "$DOWNLOAD_LOG"; }
"""
        for arch, target in [("X64", "x86_64"), ("ARM64", "aarch64"), ("X86", None)]:
            with self.subTest(arch=arch):
                installer = self.write("rustup-init.exe", "#!/bin/bash\nexit 0\n")
                installer.chmod(0o755)
                log = self.root / (arch + ".log")
                result = subprocess.run(
                    ["/bin/bash", "-c", prefix + script],
                    cwd=self.root,
                    env={
                        **os.environ,
                        "RUNNER_OS": "Windows",
                        "RUNNER_ARCH": arch,
                        "USERPROFILE": str(self.root),
                        "GITHUB_PATH": str(self.root / "github-path"),
                        "DOWNLOAD_LOG": str(log),
                    },
                    capture_output=True,
                    text=True,
                )
                if target:
                    self.assertEqual(0, result.returncode, result.stderr)
                    self.assertIn(f"/{target}-pc-windows-msvc/rustup-init.exe", log.read_text())
                else:
                    self.assertNotEqual(0, result.returncode)
                    self.assertIn("Unsupported Windows architecture", result.stderr)
                    self.assertFalse(log.exists())

    def test_rustup_download_is_confined_to_canonical_action(self):
        run = "curl -sSf https://sh.rustup.rs | sh -s -- -y"
        self.action([{"run": run}], RUST)
        self.assertEqual([], self.lint())
        self.workflow([{"run": run}])
        self.assertEqual(["tool-install"], self.codes())
        self.action([{"run": "curl -LsSf https://astral.sh/uv/install.sh | sh"}], RUST)
        self.assertEqual(2, self.codes().count("tool-install"))

    def test_excluded_v0_file_is_explicit_and_new_files_remain_covered(self):
        old = ".github/workflows/old.yml"
        self.workflow([{"uses": "actions/setup-node@v7"}], old)
        exclusions = {old: "Builds engine v0 client"}
        self.assertEqual([], self.lint(exclusions))
        self.workflow([{"uses": "actions/setup-node@v7"}], ".github/workflows/old-copy.yaml")
        self.action([{"run": "uv tool install ruff"}], ".github/actions/new/deeper/action.yml")
        self.action([{"run": "cargo install cross"}], "tools/target/new/action.yaml")
        issues = self.lint(exclusions)
        self.assertEqual(3, len(issues))
        self.assertNotIn(old, [d.path for d in issues])

    def test_stale_and_broad_exclusions_are_errors(self):
        self.workflow([{"run": "true"}])
        for exclusions in [
            {".github/workflows/*": "v0"},
            {".github/workflows/deleted.yml": "v0"},
            {".github/workflows/future.yml": ""},
        ]:
            with self.subTest(exclusions=exclusions):
                self.assertIn("scope", self.codes(exclusions))

    def test_new_v1_workflow_cannot_use_excluded_action(self):
        path = ".github/actions/old/action.yml"
        self.action([{"uses": "actions/setup-node@v7"}], path)
        self.workflow([{"uses": "./.github/actions/old"}])
        self.assertEqual(["v0-action"], self.codes({path: "v0 setup"}))

    def test_excluded_shared_workflow_does_not_exclude_other_jobs(self):
        old = ".github/workflows/docs.yml"
        self.workflow([{"uses": "actions/setup-node@v7"}], old)
        self.write(
            ".github/workflows/new.yml",
            {
                "on": "push",
                "jobs": {
                    "docs": {"uses": "./" + old},
                    "new": {
                        "runs-on": "ubuntu-latest",
                        "steps": [{"uses": "actions/setup-node@v7"}],
                    },
                },
            },
        )
        self.assertEqual(["tool-action"], self.codes({old: "shared v0 docs"}))

    def test_nested_local_actions_are_inspected_even_if_unused(self):
        self.action([{"uses": "Swatinem/rust-cache@v2"}], "deep/nested/action.yaml")
        self.assertEqual(["tool-action"], self.codes())

    def test_new_executable_action_requires_review(self):
        for runtime in ["node24", "docker"]:
            self.write(
                "new/action.yml", {"name": "new", "runs": {"using": runtime, "main": "index.js"}}
            )
            self.assertEqual(["action-runtime"], self.codes())

    def test_unknown_local_reference_is_reported(self):
        self.workflow([{"uses": "./missing"}, {"uses": "./../outside"}])
        self.assertEqual(["local-action", "local-action"], self.codes())

    def test_valid_yaml_alias_steps_are_checked(self):
        self.write(
            ".github/workflows/alias.yml",
            "on: push\njobs:\n  a:\n    steps: &steps\n      - uses: actions/setup-node@v7\n  b:\n    steps: *steps\n",
        )
        self.assertEqual(2, self.codes().count("tool-action"))

    def test_invalid_yaml_and_hidden_duplicates_fail(self):
        for source in [
            "jobs: [",
            "jobs: {}\njobs: {}",
            "jobs: &jobs {a: *jobs}",
            "jobs:\n  x:\n    <<: {steps: []}",
            "[]",
            "",
            "jobs: nope",
            "jobs: {x: {steps: nope}}",
        ]:
            with self.subTest(source=source):
                self.write(".github/workflows/broken.yaml", source)
                self.assertTrue(self.lint())

    def test_rust_compilation_requires_prior_cache_in_same_job(self):
        self.canonical()
        self.workflow([{"run": "cargo check"}, {"uses": "./.github/actions/setup-sccache"}])
        self.assertEqual(["r2-required"], self.codes())
        self.workflow(
            [{"uses": "./.github/actions/setup-sccache"}, {"run": "cargo +nightly test --locked"}]
        )
        self.assertEqual([], self.lint())
        self.workflow(
            [{"uses": "./.github/actions/setup-sccache", "if": "false"}, {"run": "cross build"}]
        )
        self.assertEqual(["r2-required"], self.codes())

    def test_canonical_composite_propagates_r2(self):
        self.canonical()
        self.action([{"uses": "./.github/actions/setup-sccache"}], RUST)
        self.workflow([{"uses": "./.github/actions/setup-rust"}, {"run": "wasm-pack build"}])
        self.assertEqual([], self.lint())

    def test_existing_direnv_convention_is_supported(self):
        self.canonical()
        self.workflow(
            [
                {
                    "uses": "./.github/actions/setup-mise",
                    "with": {"install_args": "sccache direnv"},
                },
                {"run": 'direnv allow .envrc\ndirenv export gha >> "$GITHUB_ENV"'},
                {"run": "cargo test"},
            ]
        )
        self.assertEqual([], self.lint())

    def test_rust_cache_paths_and_restore_save_are_banned(self):
        for action in ["actions/cache", "actions/cache/restore", "actions/cache/save"]:
            for path in [
                "~/.cargo/registry",
                "~/.cargo/git",
                "baml_language/target",
                "**/target/**",
                "${{ env.CARGO_TARGET_DIR }}",
                "~/.cache/sccache",
                "baml_language\\target",
            ]:
                with self.subTest(action=action, path=path):
                    self.workflow(
                        [{"uses": action + "@v6", "with": {"path": path, "key": "cache"}}]
                    )
                    self.assertIn("rust-cache", self.codes())

    def test_non_rust_dependency_caches_remain_allowed(self):
        for path in [
            "~/.cache/pre-commit",
            "~/.nuget/packages",
            "typescript2/node_modules",
            "${{ github.workspace }}/.pnpm-store",
        ]:
            self.workflow([{"uses": "actions/cache@v6", "with": {"path": path, "key": "cache"}}])
            self.assertEqual([], self.lint())

    def test_installing_tools_is_not_the_same_as_using_tools(self):
        allowed = [
            "uv run --frozen python lint.py",
            "uv sync --frozen",
            "uv pip install -r requirements.txt",
            "pip install ./dist/package.whl",
            "pnpm install --frozen-lockfile",
            "npm ci",
            "npm install --ignore-scripts",
            "pnpm exec playwright install chromium",
            "npx playwright install chromium",
            "mise run lint",
            "rustup toolchain install nightly --component miri",
            "rustup target add wasm32-unknown-unknown",
            "go test ./...",
            "cargo fetch",
            "sccache --show-stats",
            "curl https://example.com/release.json",
            'echo "cargo install cross"',
            "# uv tool install ruff",
            "cat <<'EOF'\ncargo install cross\nEOF",
        ]
        for run in allowed:
            with self.subTest(run=run):
                self.workflow([{"run": run}])
                self.assertEqual([], self.lint())

    def test_cargo_global_options_do_not_hide_builds_or_installers(self):
        for prefix in [
            "cargo --color always",
            "cargo +nightly -Z unstable-options",
            "cargo --config net.retry=10",
            "cross --color=always",
            "cargo +stable -C workspace",
        ]:
            with self.subTest(prefix=prefix):
                self.workflow([{"run": prefix + " build"}])
                self.assertIn("r2-required", self.codes())
                if prefix.startswith("cargo"):
                    self.workflow([{"run": prefix + " install cross"}])
                    self.assertIn("tool-install", self.codes())
        self.workflow([{"run": "cargo --color always fetch"}])
        self.assertEqual([], self.lint())

    def test_install_commands_shell_syntax_and_wrappers(self):
        banned = [
            "cargo install cross",
            "cargo +stable install --locked wasm-pack",
            "cargo binstall cargo-nextest",
            "cargo-binstall cross",
            "uv tool install ruff",
            "uv python install 3.13",
            "python -m pip install uv",
            "pip3 install maturin==1.0",
            "npm install -g pnpm",
            "npm --global install pnpm",
            "pnpm add --global uv",
            "corepack enable",
            "go install example.com/tool@v1",
            "mise install node",
            "mise use python@3.13",
            "dotnet tool install foo",
            "sudo -E env CI=1 cargo install cross",
            "true && cargo install cross",
            "if true; then cargo install cross; fi",
            "bash -lc 'uv tool install ruff'",
            'echo "$(cargo install cross)"',
            "cargo \\\n install cross",
            '"/usr/bin/cargo" install cross',
        ]
        for run in banned:
            with self.subTest(run=run):
                self.workflow([{"run": run}])
                self.assertIn("tool-install", self.codes())

    def test_diagnostic_points_to_file_job_step_and_command(self):
        self.write(
            ".github/workflows/new.yml",
            "on: push\njobs:\n  test:\n    steps:\n      - name: Bad installer\n        run: |\n          echo start\n          uv tool install ruff\n",
        )
        (issue,) = self.lint()
        self.assertEqual(8, issue.line)
        self.assertIn("job test, step 1 (Bad installer)", str(issue))
        self.assertIn("setup-mise", issue.message)

    def test_matrix_script_installers_are_checked(self):
        self.workflow(
            [{"run": "${{ matrix.script }}"}],
            strategy={
                "matrix": {
                    "include": [{"script": "echo harmless"}, {"script": "uv tool install ruff"}]
                }
            },
        )
        self.assertIn("tool-install", self.codes())
        self.workflow(
            [{"run": "${{ matrix._.before }}"}],
            strategy={"matrix": {"_": [{"before": "cargo install cross"}]}},
        )
        self.assertIn("tool-install", self.codes())

    def test_unresolved_executable_expression_fails_closed(self):
        self.workflow([{"run": "${{ inputs.script }}"}])
        self.assertEqual(["dynamic-run"], self.codes())

    def test_empty_canonical_action_cannot_claim_r2_setup(self):
        self.action([{"run": "echo no cache"}], R2)
        self.workflow([{"uses": "./.github/actions/setup-sccache"}, {"run": "cargo build"}])
        self.assertEqual(["r2-required"], self.codes())

    def test_raw_direnv_must_load_repo_and_export_to_actions(self):
        for run in [
            "direnv allow elsewhere\ndirenv export gha",
            "direnv allow .envrc\ndirenv export gha",
            'echo "direnv allow .envrc; direnv export gha >> $GITHUB_ENV"',
        ]:
            self.workflow([{"run": run}, {"run": "cargo build"}])
            self.assertIn("r2-required", self.codes())

    def test_canonical_cache_file_still_checks_unrelated_rust_commands(self):
        self.action([{"run": "cargo test"}], R2)
        self.assertEqual(["r2-required"], self.codes())

    def test_alternative_cache_environment_is_rejected(self):
        for key in ["SCCACHE_GHA_ENABLED", "SCCACHE_BUCKET", "SCCACHE_ENDPOINT"]:
            self.workflow([{"run": "true", "env": {key: "other"}}])
            self.assertEqual(["cache-config"], self.codes())

    def test_mise_auto_install_cannot_be_reenabled(self):
        self.workflow([{"run": "mise run test", "env": {"MISE_TASK_RUN_AUTO_INSTALL": "true"}}])
        self.assertEqual(["mise-config"], self.codes())

    def test_maturin_build_does_not_enable_an_alternate_rust_cache(self):
        self.canonical()
        self.workflow(
            [
                {"uses": "./.github/actions/setup-sccache"},
                {
                    "uses": "PyO3/maturin-action@v1",
                    "with": {"command": "build", "sccache": "false"},
                },
            ]
        )
        self.assertEqual([], self.lint())
        self.workflow([{"uses": "PyO3/maturin-action@v1", "with": {"sccache": "true"}}])
        self.assertIn("wheel-build", self.codes())
        self.assertIn("r2-required", self.codes())

    def test_malformed_fields_report_diagnostics_instead_of_crashing(self):
        for step in [
            {"uses": ["bad"]},
            {"run": ["bad"]},
            {"uses": "actions/cache@v6", "with": {"path": ["target"]}},
        ]:
            with self.subTest(step=step):
                self.workflow([step])
                self.assertIn("structure", self.codes())

    def test_r2_setup_requires_optional_credential_mapping(self):
        self.canonical()
        self.workflow([{"uses": "./.github/actions/setup-sccache"}, {"run": "cargo build"}])
        path = self.root / ".github/workflows/future.yml"
        data = yaml.safe_load(path.read_text())
        data.pop("env")
        self.write(".github/workflows/future.yml", data)
        self.assertEqual(2, self.codes().count("r2-credentials"))

    def test_raw_r2_setup_requires_mise_tools(self):
        self.workflow([{"run": 'direnv allow .envrc\ndirenv export gha >> "$GITHUB_ENV"'}])
        self.assertEqual(["r2-tools"], self.codes())

    def test_wrapper_override_cannot_disable_rust_cache(self):
        self.workflow([{"run": "true", "env": {"RUSTC_WRAPPER": ""}}])
        self.assertEqual(["cache-config"], self.codes())

    def test_cli_formats_and_failure_exit_status(self):
        for output_format in ["text", "json", "github"]:
            self.workflow([{"uses": "actions/setup-node@v7"}])
            output = io.StringIO()
            with redirect_stdout(output):
                status = main(["--root", str(self.root), "--format", output_format])
            self.assertTrue(status)
            if output_format == "json":
                self.assertTrue(
                    any(d["rule"] == "tool-action" for d in json.loads(output.getvalue()))
                )
            elif output_format == "github":
                self.assertIn("::error file=.github/workflows/future.yml", output.getvalue())
            else:
                self.assertIn("tool-action:", output.getvalue())

    def test_setup_mise_exports_explicit_version_paths_without_installing(self):
        action = yaml.safe_load((HERE.parents[1] / MISE).read_text())
        step = next(
            s
            for s in action["runs"]["steps"]
            if s.get("name") == "Export explicitly requested tool paths"
        )
        fake_mise = self.write(
            "bin/mise",
            '#!/bin/bash\nset -eu\ntest "$1" = bin-paths\nshift\nfor tool in "$@"; do printf "/tools/%s/bin\\n" "$tool"; done\n',
        )
        fake_mise.chmod(0o755)
        output = self.root / "github-path"
        subprocess.run(
            ["bash", "-euo", "pipefail", "-c", step["run"]],
            check=True,
            env={
                **os.environ,
                "PATH": str(fake_mise.parent) + os.pathsep + os.environ["PATH"],
                "REQUESTED_TOOLS": "node@24 python@3.10 npm:pnpm@9.12.0",
                "GITHUB_PATH": str(output),
            },
        )
        self.assertEqual(
            ["/tools/node@24/bin", "/tools/python@3.10/bin", "/tools/npm:pnpm@9.12.0/bin"],
            output.read_text().splitlines(),
        )

    def test_repository_exclusions_and_policy_pass(self):
        self.assertEqual([], Linter(HERE.parents[1]).run())
        exclusions = json.loads((HERE / "exclusions.json").read_text())
        self.assertEqual(27, len(exclusions))
        self.assertNotIn(".github/workflows/ci.yaml", exclusions)
        self.assertNotIn(".github/workflows/developer-docs.yml", exclusions)


if __name__ == "__main__":
    unittest.main()
