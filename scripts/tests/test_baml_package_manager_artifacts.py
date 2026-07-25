from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
GENERATOR = ROOT / "scripts" / "baml-package-manager-artifacts"
VERSION = "1.2.3"
SOURCE_SHA256 = "a" * 64
EMPTY_SHA256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"


class PackageManagerArtifactsTest(unittest.TestCase):
    def test_generates_source_formula_and_no_self_update_aur_packages(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            temp_path = Path(temp)
            wrappers = temp_path / "wrappers"
            output = temp_path / "output"
            wrappers.mkdir()
            for target in (
                "aarch64-unknown-linux-gnu",
                "x86_64-unknown-linux-gnu",
            ):
                (
                    wrappers / f"baml-wrapper-no-self-update-{VERSION}-{target}.tar.gz"
                ).touch()

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
            self.assertIn(f"archive/refs/tags/baml-wrapper-{VERSION}.tar.gz", formula)
            self.assertIn(f'sha256 "{SOURCE_SHA256}"', formula)
            self.assertIn(
                r"regex(/^baml-wrapper[._-]v?(\d+(?:\.\d+)+)$/i)", formula
            )
            self.assertIn('depends_on "rust" => :build', formula)
            self.assertIn('features: "no-self-update"', formula)
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
            self.assertNotIn("on_macos", formula)
            self.assertNotIn("/releases/download/", formula)
            subprocess.run(
                ["ruby", "-c", output / "homebrew/Formula/baml.rb"], check=True
            )

            aur_source = (output / "aur/baml/PKGBUILD").read_text()
            self.assertIn("--features no-self-update", aur_source)
            self.assertIn(SOURCE_SHA256, aur_source)

            aur_bin = (output / "aur/baml-bin/PKGBUILD").read_text()
            self.assertIn(f"baml-wrapper-no-self-update-{VERSION}", aur_bin)
            self.assertIn(EMPTY_SHA256, aur_bin)


if __name__ == "__main__":
    unittest.main()
