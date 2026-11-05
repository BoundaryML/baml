from __future__ import annotations

import hashlib
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
GENERATOR = ROOT / "scripts" / "baml-package-manager-artifacts"
VERSION = "1.2.3"
SOURCE_SHA256 = "a" * 64
SOURCE_COMMIT = "b" * 40


class PackageManagerArtifactsTest(unittest.TestCase):
    def test_generates_binary_formula_and_no_self_update_aur_packages(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            temp_path = Path(temp)
            wrappers = temp_path / "wrappers"
            output = temp_path / "output"
            wrappers.mkdir()
            targets = (
                "aarch64-apple-darwin",
                "x86_64-apple-darwin",
                "aarch64-unknown-linux-gnu",
                "x86_64-unknown-linux-gnu",
            )
            archive_shas = {}
            for target in targets:
                content = target.encode()
                archive = (
                    wrappers / f"baml-wrapper-no-self-update-{VERSION}-{target}.tar.gz"
                )
                archive.write_bytes(content)
                archive_shas[target] = hashlib.sha256(content).hexdigest()
            for target in (
                "aarch64-pc-windows-msvc",
                "x86_64-pc-windows-msvc",
            ):
                (
                    wrappers / f"baml-wrapper-no-self-update-{VERSION}-{target}.zip"
                ).write_bytes(target.encode())

            subprocess.run(
                [
                    GENERATOR,
                    "--wrapper-dir",
                    wrappers,
                    "--version",
                    VERSION,
                    "--source-sha256",
                    SOURCE_SHA256,
                    "--source-commit",
                    SOURCE_COMMIT,
                    "--out",
                    output,
                ],
                cwd=ROOT,
                check=True,
            )

            formula = (output / "homebrew/Formula/baml.rb").read_text()
            self.assertNotRegex(formula, r"(?m)^\s*version\s")
            self.assertNotIn("livecheck do", formula)
            self.assertIn("on_macos do", formula)
            self.assertIn("on_linux do", formula)
            for target in targets:
                self.assertIn(
                    f"baml-wrapper-no-self-update-{VERSION}-{target}.tar.gz",
                    formula,
                )
                self.assertIn(f'sha256 "{archive_shas[target]}"', formula)
            self.assertIn('if (buildpath/"bin/baml").exist?', formula)
            self.assertIn('bin.install "bin/baml"', formula)
            self.assertIn('bin.install "baml"', formula)
            self.assertIn("test do", formula)
            self.assertIn(
                f'assert_match "baml wrapper {VERSION}"', formula
            )
            self.assertIn('shell_output("#{bin}/baml --version")', formula)
            self.assertIn('ENV["BAML_TOOLCHAIN"] = (testpath/"baml-cli").to_s', formula)
            self.assertIn('assert_match "Homebrew CLI fixture", version_output', formula)
            self.assertNotIn('ENV.delete "BAML_TOOLCHAIN"', formula)
            self.assertIn('assert_match "installed toolchains: (none)"', formula)
            self.assertIn('shell_output("#{bin}/baml toolchain list")', formula)
            self.assertIn("self-update is disabled in this build", formula)
            self.assertNotIn('"toolchain", "use"', formula)
            self.assertNotIn('system bin/"baml", "init"', formula)
            self.assertNotIn('system bin/"baml", "check"', formula)
            self.assertNotIn('toolchains/1.2.3', formula)
            self.assertNotIn("archive/refs/tags", formula)
            self.assertNotIn(f'sha256 "{SOURCE_SHA256}"', formula)
            self.assertNotIn('depends_on "rust"', formula)
            self.assertNotIn('system "cargo"', formula)
            self.assertNotIn("windows-msvc", formula)
            subprocess.run(
                ["ruby", "-c", output / "homebrew/Formula/baml.rb"], check=True
            )

            aur_source = (output / "aur/baml/PKGBUILD").read_text()
            self.assertIn("--bin baml-cli --bin baml-pack-host", aur_source)
            self.assertIn("python wrapper/build", aur_source)
            self.assertIn("--no-self-update", aur_source)
            self.assertIn(f"--version {VERSION}", aur_source)
            self.assertIn("makedepends = python", (output / "aur/baml/.SRCINFO").read_text())
            self.assertIn(SOURCE_SHA256, aur_source)
            self.assertIn(f"export BAML_GIT_SHA={SOURCE_COMMIT}", aur_source)

            aur_bin = (output / "aur/baml-bin/PKGBUILD").read_text()
            self.assertIn(f"baml-wrapper-no-self-update-{VERSION}", aur_bin)
            self.assertIn(
                archive_shas["aarch64-unknown-linux-gnu"], aur_bin
            )
            self.assertIn(
                archive_shas["x86_64-unknown-linux-gnu"], aur_bin
            )
            self.assertNotIn("windows-msvc", aur_bin)

    def test_source_archive_build_uses_the_release_commit_without_git_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            recipe = root / "PKGBUILD"
            recipe.write_text(
                (ROOT / "packaging/aur/baml/PKGBUILD").read_text()
                .replace("__VERSION_ORIG__", VERSION)
                .replace("__VERSION__", VERSION)
                .replace("__SOURCE_COMMIT__", SOURCE_COMMIT)
                .replace("__SHA256__", SOURCE_SHA256)
            )
            (root / f"baml-baml-wrapper-{VERSION}/baml_language").mkdir(parents=True)
            result = subprocess.run(
                ["bash", "-c", '''
set -euo pipefail
source "$1"
cargo() { printf 'cargo %s commit=%s toolchain=%s target=%s\n' "$*" "$BAML_GIT_SHA" "$RUSTUP_TOOLCHAIN" "$CARGO_TARGET_DIR"; }
python() { printf 'python %s commit=%s\n' "$*" "$BAML_GIT_SHA"; }
build
''', "test-pkgbuild", str(recipe)],
                cwd=root, env={**os.environ, "BAML_GIT_SHA": "unrelated-ambient-commit"},
                check=True, capture_output=True, text=True,
            )
            self.assertIn(f"cargo build --release --frozen --bin baml-cli --bin baml-pack-host commit={SOURCE_COMMIT} toolchain=stable target=target", result.stdout)
            self.assertIn(f"python wrapper/build --cli target/release/baml-cli --pack-host target/release/baml-pack-host --version {VERSION} --no-self-update --output target/release/baml commit={SOURCE_COMMIT}", result.stdout)

    def test_rejects_an_invalid_source_commit_before_generating_packages(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "output"
            result = subprocess.run(
                [GENERATOR, "--wrapper-dir", root, "--version", VERSION,
                 "--out", output, "--source-commit", "not-a-commit"],
                cwd=ROOT, capture_output=True, text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("--source-commit must be a full lowercase Git commit id", result.stderr)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
