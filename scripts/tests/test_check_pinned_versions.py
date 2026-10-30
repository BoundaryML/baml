from __future__ import annotations

import functools
import importlib.machinery
import importlib.util
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CHECK_TOOL = ROOT / "scripts" / "check-pinned-versions"


@functools.cache
def check_module():
    """Import the tool; it has no `.py` suffix, so it needs an explicit loader."""
    loader = importlib.machinery.SourceFileLoader(
        "check_pinned_versions", str(CHECK_TOOL)
    )
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    sys.modules[loader.name] = module
    loader.exec_module(module)
    return module


def messages(yaml: str) -> list[tuple[int, str]]:
    violations = check_module().check_text(Path("workflow.yml"), textwrap.dedent(yaml))
    return [(violation.line, violation.message) for violation in violations]


class VersionInputTests(unittest.TestCase):
    def test_floating_values_are_rejected(self) -> None:
        for value in [
            "latest",
            "'latest'",
            '"stable"',
            "nightly",
            "beta",
            "lts/*",
            "lts/iron",
            "current",
            "node",
            "'*'",
            "x",
        ]:
            with self.subTest(value=value):
                found = messages(f"""
                    - uses: actions/setup-node@v7
                      with:
                        node-version: {value}
                """)
                self.assertEqual(len(found), 1, found)
                self.assertEqual(found[0][0], 4)

    def test_pinned_values_are_accepted(self) -> None:
        for value in [
            "26.13.0",
            "'24'",
            '"3.13"',
            "1.98.0",
            "nightly-2026-10-29",
            "${{ inputs.node-version }}",
            "${{ env.NODE_VERSION }}",
            "temurin-23",
        ]:
            with self.subTest(value=value):
                self.assertEqual(messages(f"node-version: {value}"), [])

    def test_any_version_or_toolchain_key(self) -> None:
        for key in [
            "python-version",
            "go-version",
            "java-version",
            "dotnet-version",
            "version",
            "toolchain",
            "rust-toolchain",
            "rustup-toolchain",
            "NODE_VERSION",
            "rubygems",
        ]:
            with self.subTest(key=key):
                self.assertEqual(len(messages(f"{key}: latest")), 1)

    def test_unrelated_keys_are_ignored(self) -> None:
        self.assertEqual(messages("ref: main\nchannel: stable\nversioning: latest"), [])

    def test_composite_input_default(self) -> None:
        found = messages("""
            inputs:
              rust-toolchain:
                description: "Rust toolchain version"
                required: false
                default: "stable"
              other:
                default: "stable"
        """)
        self.assertEqual(
            found,
            [
                (
                    6,
                    "input `rust-toolchain` defaults to `stable`, which floats; pin an exact version",
                )
            ],
        )

    def test_composite_input_default_pinned(self) -> None:
        self.assertEqual(
            messages("""
            inputs:
              node-version:
                default: '22.23.4'
        """),
            [],
        )

    def test_toolchain_ref_actions(self) -> None:
        self.assertEqual(len(messages("- uses: dtolnay/rust-toolchain@stable")), 1)
        self.assertEqual(len(messages("- uses: dtolnay/rust-toolchain@nightly")), 1)
        self.assertEqual(messages("- uses: dtolnay/rust-toolchain@1.98.0"), [])
        self.assertEqual(messages("- uses: actions/checkout@v7"), [])


class CommandTests(unittest.TestCase):
    def assert_flags(self, command: str) -> None:
        self.assertEqual(len(messages(f"run: {command}")), 1, command)

    def assert_allows(self, command: str) -> None:
        self.assertEqual(messages(f"run: {command}"), [], command)

    def test_global_package_installs(self) -> None:
        self.assert_flags("npm install -g fern-api")
        self.assert_flags("npm i --global @vscode/vsce")
        self.assert_flags("pnpm add -g typescript")
        self.assert_flags("yarn global add typescript")
        self.assert_allows("npm install -g fern-api@4.57.0")
        self.assert_allows('npm install -g "@napi-rs/cli@${napi_version}"')
        self.assert_allows("npm install")
        self.assert_allows("pnpm install --frozen-lockfile")

    def test_npx(self) -> None:
        self.assert_flags("npx ovsx publish")
        self.assert_flags("npx @vscode/vsce publish --packagePath x.vsix")
        self.assert_flags("pnpm dlx create-vite")
        self.assert_allows("npx --yes ovsx@1.3.0 publish")
        self.assert_allows("npx --no playwright install chromium")

    def test_cargo_install(self) -> None:
        self.assert_flags("cargo install cross")
        self.assert_flags("cargo install cross --git https://github.com/cross-rs/cross")
        self.assert_allows("cargo install cross --version 0.2.5 --locked")
        self.assert_allows("cargo install cross@0.2.5")
        self.assert_allows(
            "cargo install cross --git https://github.com/cross-rs/cross --rev d5a5ead"
        )
        self.assert_allows(
            "cargo install cross --git https://github.com/cross-rs/cross --tag v0.2.5"
        )
        self.assert_allows("cargo install --path tools/foo")

    def test_go_install(self) -> None:
        self.assert_flags("go install golang.org/x/tools/cmd/goimports@latest")
        self.assert_flags("go install golang.org/x/tools/cmd/goimports")
        self.assert_allows("go install golang.org/x/tools/cmd/goimports@v0.36.0")
        self.assert_allows("go install ./cmd/tool")

    def test_python_installs(self) -> None:
        self.assert_flags("uv tool install maturin")
        self.assert_flags("pip install ruff")
        self.assert_flags("uvx ruff check")
        self.assert_allows("uv tool install ruff==0.15.22")
        self.assert_allows("pip install -r requirements.txt")
        self.assert_allows("uv pip install $(ls dist/*.whl)")
        self.assert_allows("pip install -e .")

    def test_gem_install(self) -> None:
        self.assert_flags("gem install rb_sys")
        self.assert_allows('gem install rb_sys -v "$version"')

    def test_rustup(self) -> None:
        self.assert_flags("rustup toolchain install nightly --component miri")
        self.assert_flags("rustup override set nightly")
        self.assert_flags("rustup default stable")
        self.assert_flags("rustup toolchain install stable-x86_64-unknown-linux-gnu")
        self.assert_flags("cargo +nightly fetch")
        self.assert_allows(
            'rustup toolchain install "$MIRI_TOOLCHAIN" --component miri'
        )
        self.assert_allows(
            "rustup toolchain install nightly-2026-10-29 --component miri"
        )
        self.assert_allows("rustup toolchain install 1.97.1 --profile minimal")
        self.assert_allows("rustup toolchain install")
        self.assert_allows("rustup target add wasm32-unknown-unknown")
        self.assert_allows('cargo "+$MIRI_TOOLCHAIN" fetch')

    def test_piped_installers(self) -> None:
        self.assert_flags("curl -fsSL https://ollama.com/install.sh | sh")
        self.assert_allows(
            "curl -fsSL https://ollama.com/install.sh | OLLAMA_VERSION=0.40.13 sh"
        )

    def test_at_latest_anywhere(self) -> None:
        self.assert_flags("pnpm dlx some-tool@latest")

    def test_commands_after_separators_and_env(self) -> None:
        self.assert_flags("cd x && npm install -g fern-api")
        self.assert_flags("FOO=1 cargo install cross")
        self.assert_flags("sudo gem install rb_sys")

    def test_multiline_run_block(self) -> None:
        found = messages("""
            - run: |
                set -euo pipefail
                cargo install \\
                  cross
                echo done
        """)
        self.assertEqual([line for line, _ in found], [4])


class EscapeHatchAndCommentTests(unittest.TestCase):
    def test_float_ok_marker(self) -> None:
        self.assertEqual(
            messages(
                "run: curl https://sh.rustup.rs | sh -s -- -y # float-ok: rustup bootstrap"
            ),
            [],
        )

    def test_float_ok_needs_a_reason(self) -> None:
        self.assertEqual(len(messages("node-version: latest # float-ok:")), 1)

    def test_comments_are_ignored(self) -> None:
        self.assertEqual(
            messages("#   node-version: latest\n# cargo install cross"), []
        )
        self.assertEqual(messages("node-version: '26.13.0' # not latest"), [])


class RepositoryTests(unittest.TestCase):
    def test_repository_workflows_are_pinned(self) -> None:
        result = subprocess.run(
            [sys.executable, str(CHECK_TOOL)], capture_output=True, text=True, cwd=ROOT
        )
        self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
