import io
import tempfile
import unittest
from pathlib import Path
import tarfile
import zipfile

from release_artifacts import candidates_from_archives


class ReleaseArtifactsTests(unittest.TestCase):
    def stage(self, root, windows=False, disabled=True, symlink=False):
        target = "x86_64-pc-windows-msvc" if windows else "aarch64-unknown-linux-musl"
        platform = {
            "triple": target,
            "os": "windows" if windows else "linux",
            "archive_suffix": ".zip" if windows else ".tar.gz",
            "artifacts": {
                "wrapper": {
                    "variants": ["self-update"]
                    + (["no-self-update"] if disabled else [])
                }
            },
        }
        for product in ["baml-wrapper", "baml-wrapper-baml"]:
            for variant in platform["artifacts"]["wrapper"]["variants"]:
                name = product + ("" if variant == "self-update" else "-no-self-update")
                archive = root / f"{name}-0.2.5-{target}{platform['archive_suffix']}"
                content = name.encode()
                if windows:
                    with zipfile.ZipFile(archive, "w") as source:
                        source.writestr("bin/baml.exe", content)
                else:
                    with tarfile.open(archive, "w:gz") as source:
                        member = tarfile.TarInfo("./bin/baml")
                        member.size = len(content)
                        if symlink:
                            member.type = tarfile.SYMTYPE
                            member.linkname = "../../outside"
                            member.size = 0
                        source.addfile(member, io.BytesIO(content))
        return platform

    def test_uploaded_bytes_and_variants(self):
        for windows, disabled in [(False, False), (False, True), (True, True)]:
            with (
                self.subTest(windows=windows, disabled=disabled),
                tempfile.TemporaryDirectory() as temporary,
            ):
                root = Path(temporary)
                platform = self.stage(root, windows, disabled)
                candidates = candidates_from_archives(root, root, platform, "0.2.5")
                self.assertEqual(len(candidates), 4 if disabled else 2)
                for candidate in candidates:
                    product = (
                        "baml-wrapper-baml"
                        if candidate["label"].startswith("baml")
                        else "baml-wrapper"
                    )
                    if not candidate["self_update"]:
                        product += "-no-self-update"
                    self.assertEqual(
                        Path(candidate["command"][0]).read_bytes(), product.encode()
                    )
                    self.assertEqual(candidate["version"], "0.2.5")

    def test_missing_shipped_variant_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            platform = self.stage(root)
            next(root.glob("baml-wrapper-baml-no-self-update-*")).unlink()
            with self.assertRaisesRegex(ValueError, "expected exactly one"):
                candidates_from_archives(root, root, platform, "0.2.5")

    def test_unknown_shipped_variant_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            platform = self.stage(root)
            platform["artifacts"]["wrapper"]["variants"].append("unexpected")
            with self.assertRaisesRegex(ValueError, "unexpected"):
                candidates_from_archives(root, root, platform, "0.2.5")

    def test_symlink_is_not_an_executable(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            platform = self.stage(root, symlink=True)
            with self.assertRaisesRegex(ValueError, "regular bin/baml"):
                candidates_from_archives(root, root, platform, "0.2.5")


if __name__ == "__main__":
    unittest.main()
