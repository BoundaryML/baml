from __future__ import annotations

import json
import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
VERSION_TOOL = ROOT / "scripts" / "baml-language-version"


class VersionToolTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.make_fixture("1.2.3")
        self.env = {
            **os.environ,
            "BAML_LANGUAGE_VERSION_ROOT": str(self.root),
            "BAML_LANGUAGE_RELEASED_AT": "2026-07-23T12:34:56Z",
            "BAML_LANGUAGE_VERSION_DATE": "20260723",
            "BAML_LANGUAGE_VERSION_NIGHTLY_LETTER": "h",
            "PATH": f"{self.bin}:{os.environ['PATH']}",
        }

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write(self, relative: str, content: str) -> Path:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        return path

    def make_fixture(self, version: str) -> None:
        self.write(
            "baml_language/release.toml",
            f'[release]\ncanary_version = "{version}"\n',
        )
        self.write(
            "baml_language/crates/baml_version/src/lib.rs",
            "\n".join(
                [
                    f'pub const CANONICAL_VERSION: &str = "{version}";',
                    f'pub const PYPI_VERSION: &str = "{version}";',
                    'pub const CHANNEL: &str = "canary";',
                    f'pub const STABLE_VERSION: &str = "{version}";',
                    "",
                ]
            ),
        )
        self.write(
            "baml_language/sdks/python/pyproject.toml",
            f'[project]\nversion = "{version}"\n',
        )
        self.write(
            "baml_language/sdks/python/src/baml_bridge/__init__.py",
            f'__version__ = "{version}"\n',
        )
        for package in (
            "baml_language/sdks/typescript/bridge_typescript/package.json",
            "baml_language/sdks/typescript/bridge_typescript_web/package.json",
            "typescript2/app-vscode-ext/package.json",
        ):
            self.write(
                package,
                json.dumps({"version": version, "name": "fixture"}, indent=2)
                + "\n",
            )
        self.write(
            "baml_language/sdks/typescript/bridge_typescript/dist/native.js",
            f"if (bindingPackageVersion !== '{version}') {{ throw new Error(); }}\n",
        )
        self.write(
            "baml_language/sdks/go/baml_go/version.go",
            f'const DefaultRuntimeVersion = "{version}"\n',
        )
        self.write(
            "baml_language/sdks/rust/bridge_rust/src/version.rs",
            f'pub(crate) const CANONICAL_VERSION: &str = "{version}";\n',
        )
        self.write(
            "baml_language/sdks/rust/bridge_rust/Cargo.toml",
            f'[package]\nversion = "{version}"\n',
        )
        self.write(
            "baml_language/crates/baml/Cargo.toml",
            '[package]\nversion = "1.0.0"\n',
        )
        self.write(
            "baml_language/sdks/csharp/bridge_csharp/src/Baml.Bridge.csproj",
            f"<Project>\n  <PropertyGroup>\n    <Version>{version}</Version>\n"
            "  </PropertyGroup>\n</Project>\n",
        )
        self.write(
            "baml_language/sdks/java/baml_bridge/build.gradle.kts",
            "val bamlVersion = \"fixture\"\nversion = bamlVersion\n",
        )
        self.write(
            "baml_language/sdks/java/baml-bridge-kotlin/build.gradle.kts",
            "val bamlVersion = \"fixture\"\nversion = bamlVersion\n"
            'dependencies {\n    api("com.boundaryml:baml-bridge:$bamlVersion")\n}\n',
        )
        self.write(
            "baml_language/sdks/java/gradle-plugin/build.gradle.kts",
            "val bamlVersion = \"fixture\"\nversion = bamlVersion\n",
        )
        fake_pnpm = self.write(
            "bin/pnpm",
            """#!/usr/bin/env python3
import json
import pathlib
import sys

if "build:debug" in sys.argv:
    root = pathlib.Path.cwd()
    version = json.loads((root / "package.json").read_text())["version"]
    output = root / "dist" / "native.js"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        f"if (bindingPackageVersion !== '{version}') {{ throw new Error(); }}\\n"
    )
""",
        )
        fake_pnpm.chmod(0o755)

    def run_tool(
        self,
        *args: str,
        check: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(VERSION_TOOL), *args],
            cwd=self.root,
            env=self.env,
            check=check,
            text=True,
            capture_output=True,
        )

    def plan(self, channel: str) -> dict:
        path = self.root / f"{channel}-plan.json"
        self.run_tool("plan", "--channel", channel, "--out", str(path))
        return json.loads(path.read_text(encoding="utf-8"))

    def surface_versions(self) -> dict[str, str]:
        def json_version(relative: str) -> str:
            return json.loads((self.root / relative).read_text())["version"]

        def match(relative: str, pattern: str) -> str:
            text = (self.root / relative).read_text(encoding="utf-8")
            result = re.search(pattern, text, flags=re.MULTILINE)
            self.assertIsNotNone(result, relative)
            return result.group(1)

        return {
            "python": match(
                "baml_language/sdks/python/pyproject.toml",
                r'^version = "([^"]+)"$',
            ),
            "node": json_version(
                "baml_language/sdks/typescript/bridge_typescript/package.json"
            ),
            "web": json_version(
                "baml_language/sdks/typescript/bridge_typescript_web/package.json"
            ),
            "rust": match(
                "baml_language/sdks/rust/bridge_rust/Cargo.toml",
                r'^version = "([^"]+)"$',
            ),
            "go": match(
                "baml_language/sdks/go/baml_go/version.go",
                r'^const DefaultRuntimeVersion = "([^"]+)"$',
            ),
            "csharp": match(
                "baml_language/sdks/csharp/bridge_csharp/src/Baml.Bridge.csproj",
                r"^    <Version>([^<]+)</Version>$",
            ),
            "vsix": json_version("typescript2/app-vscode-ext/package.json"),
        }

    def test_set_bump_sync_check_and_registry_plan(self) -> None:
        self.run_tool("sync")
        self.run_tool("check")
        self.run_tool("set", "2.3.4")
        self.assertEqual(set(self.surface_versions().values()), {"2.3.4"})
        self.run_tool("check")
        self.run_tool("bump", "--patch")
        self.assertEqual(set(self.surface_versions().values()), {"2.3.5"})
        self.run_tool("check")

        plan = self.plan("canary")
        self.assertEqual(plan["schema"], 3)
        self.assertEqual(
            plan["registry_versions"],
            {
                "pypi": "2.3.5",
                "npm": "2.3.5",
                "crates_io": "2.3.5",
                "nuget": "2.3.5",
                "maven": "2.3.5",
                "gradle_plugin": "2.3.5",
                "swiftpm": "2.3.5",
            },
        )
        self.assertEqual(plan["released_at"], "2026-07-23T12:34:56Z")

    def test_prerelease_plan_and_stamp_cover_every_shipping_sdk(self) -> None:
        plan_path = self.root / "nightly-plan.json"
        self.run_tool("plan", "--channel", "nightly", "--out", str(plan_path))
        plan = json.loads(plan_path.read_text(encoding="utf-8"))
        canonical = "1.2.4-nightly.20260723.h"
        self.assertEqual(plan["canonical_version"], canonical)
        self.assertEqual(plan["registry_versions"]["pypi"], "1.2.4.dev2026072307")
        for registry in (
            "npm",
            "crates_io",
            "nuget",
            "maven",
            "gradle_plugin",
            "swiftpm",
        ):
            self.assertEqual(plan["registry_versions"][registry], canonical)

        self.run_tool("stamp", "--plan", str(plan_path))
        versions = self.surface_versions()
        self.assertEqual(versions["python"], "1.2.4.dev2026072307")
        for sdk in ("node", "web", "rust", "go", "csharp", "vsix"):
            self.assertEqual(versions[sdk], canonical)
        # Restoring committed Canary metadata exercises sync after a
        # prerelease stamp and proves nightly values do not remain behind.
        self.run_tool("sync")
        self.run_tool("check")
        self.assertEqual(set(self.surface_versions().values()), {"1.2.3"})

    def test_exact_named_field_checks_reject_loose_version_text(self) -> None:
        rust_version = (
            self.root
            / "baml_language/sdks/rust/bridge_rust/src/version.rs"
        )
        rust_version.write_text(
            'pub(crate) const CANONICAL_VERSION: &str = "9.9.9";\n'
            'pub(crate) const DECOY: &str = "1.2.3";\n',
            encoding="utf-8",
        )
        result = self.run_tool("check", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("rust bridge CANONICAL_VERSION", result.stderr)

        self.run_tool("sync")
        csharp = (
            self.root
            / "baml_language/sdks/csharp/bridge_csharp/src/Baml.Bridge.csproj"
        )
        csharp.write_text(
            "<Project>\n  <PropertyGroup>\n    <Version>9.9.9</Version>\n"
            "    <DecoyVersion>1.2.3</DecoyVersion>\n"
            "  </PropertyGroup>\n</Project>\n",
            encoding="utf-8",
        )
        result = self.run_tool("check", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("C# NuGet project version", result.stderr)


if __name__ == "__main__":
    unittest.main()
