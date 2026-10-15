"""Regression tests of produced containers, including failure cases."""

import argparse
import hashlib
import importlib.machinery
import importlib.util
import io
import json
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "baml-sdk-licenses"
LOADER = importlib.machinery.SourceFileLoader("sdk_licenses", str(SCRIPT))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
LICENSES = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(LICENSES)


class DistributionLicenses(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.license = (LICENSES.ROOT / "LICENSE").read_bytes()
        self.quanta = (
            LICENSES.ROOT / "baml_language/vendor/quanta/LICENSE"
        ).read_bytes()

    def archive(self, name, files):
        path = self.root / name
        if name.endswith((".tgz", ".crate", ".tar.gz")):
            with tarfile.open(path, "w:gz") as archive:
                for name, data in files.items():
                    info = tarfile.TarInfo(name)
                    info.size = len(data)
                    archive.addfile(info, io.BytesIO(data))
        else:
            with zipfile.ZipFile(path, "w") as archive:
                for name, data in files.items():
                    archive.writestr(name, data)
        return path

    def test_committed_package_license_copies_match_authoritative_sources(self):
        for relative, engine in LICENSES.SOURCES.items():
            LICENSES.verify_directory(LICENSES.ROOT / relative, engine)

    def test_npm_metadata_and_native_notice_are_both_required(self):
        files = {
            "package/LICENSE": self.license,
            "package/LICENSE.quanta": self.quanta,
            "package/package.json": json.dumps(
                {"name": "@boundaryml/baml-bridge", "license": "Apache-2.0"}
            ).encode(),
        }
        LICENSES.verify_archive(self.archive("bridge.tgz", files))
        files["package/package.json"] = (
            b'{"name":"@boundaryml/baml-bridge","license":"MIT"}'
        )
        with self.assertRaisesRegex(ValueError, "license must be"):
            LICENSES.verify_archive(self.archive("wrong.tgz", files))
        files["package/package.json"] = (
            b'{"name":"@boundaryml/baml-bridge","license":"Apache-2.0"}'
        )
        del files["package/LICENSE.quanta"]
        with self.assertRaisesRegex(ValueError, "LICENSE.quanta missing"):
            LICENSES.verify_archive(self.archive("missing-notice.tgz", files))

    def test_python_expression_and_license_file_headers(self):
        files = {
            "baml_bridge.dist-info/licenses/LICENSE": self.license,
            "baml_bridge.dist-info/licenses/LICENSE.quanta": self.quanta,
            "baml_bridge.dist-info/METADATA": b"License-Expression: Apache-2.0\nLicense-File: LICENSE\n",
        }
        LICENSES.verify_archive(self.archive("bridge.whl", files))
        files["baml_bridge.dist-info/METADATA"] = b"License-Expression: Apache-2.0\n"
        with self.assertRaisesRegex(ValueError, "Python license"):
            LICENSES.verify_archive(self.archive("wrong.whl", files))

    def test_nuget_requires_expression_and_bundled_text(self):
        files = {
            "LICENSE": self.license,
            "LICENSE.quanta": self.quanta,
            "baml-bridge.nuspec": b'<package><metadata><license type="expression">Apache-2.0</license></metadata></package>',
        }
        LICENSES.verify_archive(self.archive("bridge.nupkg", files))
        files["baml-bridge.nuspec"] = b"<package><metadata/></package>"
        with self.assertRaisesRegex(ValueError, "NuGet"):
            LICENSES.verify_archive(self.archive("wrong.nupkg", files))

    def test_third_party_javadoc_terms_do_not_substitute_for_first_party_license(self):
        files = {
            "legal/LICENSE": b"Third-party GPL terms",
            "META-INF/LICENSE": self.license,
        }
        LICENSES.verify_archive(self.archive("docs.jar", files))
        del files["META-INF/LICENSE"]
        with self.assertRaisesRegex(ValueError, "authoritative LICENSE missing"):
            LICENSES.verify_archive(self.archive("wrong.jar", files))

    def test_go_swift_rust_and_toolchain_archive_layouts(self):
        for name, prefix in [
            ("go.zip", "module@v0.1/"),
            ("swift.zip", "swift/"),
            ("sdk.crate", "sdk-0.1/"),
            ("baml-language-0.1.tar.gz", "./"),
            ("BamlBridgeFFI.xcframework.zip", "BamlBridgeFFI.xcframework/"),
        ]:
            with self.subTest(name=name):
                files = {
                    prefix + "LICENSE": self.license,
                    prefix + "LICENSE.quanta": self.quanta,
                }
                LICENSES.verify_archive(self.archive(name, files))
                files[prefix + "LICENSE"] = b"wrong terms"
                with self.assertRaises(ValueError):
                    LICENSES.verify_archive(self.archive(name, files))

    def test_vsce_license_txt_filename(self):
        files = {
            "extension/LICENSE.txt": self.license,
            "extension/LICENSE.quanta": self.quanta,
            "extension/package.json": b'{"license":"Apache-2.0"}',
        }
        LICENSES.verify_archive(self.archive("baml-language-0.1.vsix", files))

    def test_stage_preserves_distinct_component_license(self):
        (self.root / "LICENSE").write_bytes(b"legitimate different terms")
        with self.assertRaisesRegex(ValueError, "Refusing to overwrite"):
            LICENSES.stage(self.root)
        self.assertEqual(
            (self.root / "LICENSE").read_bytes(), b"legitimate different terms"
        )

    def csharp_staging_fixture(self):
        loader = importlib.machinery.SourceFileLoader(
            "csharp_contract", str(SCRIPT.parent / "baml-csharp-release-contract")
        )
        spec = importlib.util.spec_from_loader(loader.name, loader)
        contract = importlib.util.module_from_spec(spec)
        loader.exec_module(contract)
        downloads = self.root / "downloads"
        for target in contract.csharp_targets(contract.DEFAULT_PLATFORMS):
            directory = downloads / f"bridge-cffi-{target['target']}"
            directory.mkdir(parents=True)
            asset = target["producer_asset"]
            payload = target["target"].encode()
            (directory / asset).write_bytes(payload)
            (directory / (asset + ".sha256")).write_text(
                hashlib.sha256(payload).hexdigest() + "  " + asset + "\n"
            )
            for name, source in LICENSES.license_sources(engine=True).items():
                (directory / (asset + "." + name)).write_bytes(source.read_bytes())
        args = argparse.Namespace(
            downloads=downloads,
            staging=self.root / "staged",
            platforms=contract.DEFAULT_PLATFORMS,
            source_sha="a" * 40,
            release_version="0.22.2",
            workflow_run_id="123",
            provenance=self.root / "provenance.json",
            manifest=self.root / "native.sha256",
        )
        return contract, args

    def test_csharp_staging_accepts_all_eight_producers_with_license_sidecars(self):
        contract, args = self.csharp_staging_fixture()
        contract.stage_native(args)
        self.assertEqual(len(list(args.staging.glob("runtimes/*/native/*"))), 8)
        self.assertEqual(len(json.loads(args.provenance.read_text())["artifacts"]), 8)

    def test_csharp_staging_rejects_wrong_license_sidecar(self):
        contract, args = self.csharp_staging_fixture()
        sidecar = next(args.downloads.rglob("*.LICENSE"))
        sidecar.write_bytes(b"wrong terms")
        with self.assertRaisesRegex(contract.ContractError, "authoritative source"):
            contract.stage_native(args)

    def test_native_release_requires_exact_license_sidecars(self):
        native = self.root / "libbaml_cffi-aarch64-apple-darwin.dylib"
        native.write_bytes(b"fixture, never executed")
        with self.assertRaisesRegex(ValueError, "sidecar"):
            LICENSES.verify_artifacts(self.root)
        for name, source in LICENSES.license_sources(engine=True).items():
            native.with_name(native.name + "." + name).write_bytes(source.read_bytes())
        LICENSES.verify_artifacts(self.root)


if __name__ == "__main__":
    unittest.main()
