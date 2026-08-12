from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
ASSEMBLER = ROOT / "scripts" / "assemble-swift-sdk-mirror"


class SwiftReleaseContractTests(unittest.TestCase):
    def run_assembler(
        self,
        output: Path,
        *,
        check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                str(ASSEMBLER),
                "--out",
                str(output),
                "--version",
                "1.2.3",
                "--source-sha",
                "1" * 40,
                "--zip-url",
                "https://example.com/BamlBridgeFFI-1.2.3.xcframework.zip",
                "--checksum",
                "a" * 64,
            ],
            cwd=ROOT,
            check=check,
            text=True,
            capture_output=True,
        )

    def test_assembles_fresh_mirror_and_refuses_existing_destination(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "mirror"
            self.run_assembler(output)

            manifest = (output / "Package.swift").read_text(encoding="utf-8")
            self.assertIn("1.2.3", manifest)
            self.assertIn("a" * 64, manifest)
            self.assertIn("https://example.com/", manifest)
            self.assertNotIn("__VERSION__", manifest)
            self.assertEqual(
                (output / "SOURCE_SHA").read_text(encoding="utf-8"),
                "1" * 40 + "\n",
            )
            self.assertTrue((output / "Sources" / "BamlBridge").is_dir())

            sentinel = output / "do-not-delete"
            sentinel.write_text("preserve me\n", encoding="utf-8")
            result = self.run_assembler(output, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("must not already exist", result.stderr)
            self.assertEqual(sentinel.read_text(encoding="utf-8"), "preserve me\n")


if __name__ == "__main__":
    unittest.main()
