from __future__ import annotations

import hashlib
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
GENERATOR = ROOT / "scripts" / "baml-package-manager-artifacts"
VERSION = "1.2.3"
SOURCE_SHA256 = "a" * 64


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
            self.assertIn("--features no-self-update", aur_source)
            self.assertIn(SOURCE_SHA256, aur_source)

            aur_bin = (output / "aur/baml-bin/PKGBUILD").read_text()
            self.assertIn(f"baml-wrapper-no-self-update-{VERSION}", aur_bin)
            self.assertIn(
                archive_shas["aarch64-unknown-linux-gnu"], aur_bin
            )
            self.assertIn(
                archive_shas["x86_64-unknown-linux-gnu"], aur_bin
            )
            self.assertNotIn("windows-msvc", aur_bin)


if __name__ == "__main__":
    unittest.main()
