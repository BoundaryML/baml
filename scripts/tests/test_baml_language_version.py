from __future__ import annotations

import datetime as dt
import functools
import importlib.machinery
import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
import unittest.mock
import zoneinfo
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
VERSION_TOOL = ROOT / "scripts" / "baml-language-version"


@functools.cache
def version_tool_module():
    """Import the tool for tests that exercise a pure function directly.

    It has no `.py` suffix, so it needs an explicit source loader, and it must
    be in `sys.modules` before it executes for its dataclasses to resolve.
    """
    loader = importlib.machinery.SourceFileLoader(
        "baml_language_version", str(VERSION_TOOL)
    )
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    sys.modules[loader.name] = module
    loader.exec_module(module)
    return module


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
                    "#[allow(dead_code)]",
                    f'const STABLE_VERSION: &str = "{version}";',
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
                json.dumps({"name": "fixture", "version": version}, indent=2)
                + "\n",
            )
        self.write(
            "baml_language/sdks/typescript/bridge_typescript/dist/native.js",
            f"if (bindingPackageVersion !== '{version}') {{ throw new Error(); }}\n",
        )
        self.write(
            "baml_language/sdks/go/baml_go/version.go",
            f'const ToolchainVersion = "{version}"\n'
            f'const BridgeRuntimeVersion = "v{version}"\n',
        )
        self.write(
            "baml_language/sdks/rust/bridge_rust/src/version.rs",
            f'pub(crate) const TOOLCHAIN_VERSION: &str = "{version}";\n'
            f'pub(crate) const BRIDGE_RUNTIME_VERSION: &str = "{version}";\n',
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
            "baml_language/sdks/csharp/bridge_csharp/src/RuntimeIdentity.cs",
            "internal static class RuntimeIdentity\n"
            "{\n"
            f'    internal const string ToolchainVersion = "{version}";\n'
            f'    internal const string BridgeRuntimeVersion = "{version}";\n'
            "}\n",
        )
        self.write(
            "baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge/BamlVersion.java",
            "final class BamlVersion {\n"
            f'    static final String TOOLCHAIN_VERSION = "{version}";\n'
            f'    static final String BRIDGE_RUNTIME_VERSION = "{version}";\n'
            "}\n",
        )
        self.write(
            "baml_language/sdks/swift/Sources/BamlBridge/RuntimeIdentity.swift",
            "public enum BamlBridgeIdentity {\n"
            f'    public static let toolchainVersion = "{version}"\n'
            f'    public static let bridgeRuntimeVersion = "{version}"\n'
            "}\n",
        )
        self.write(
            "baml_language/sdks/cpp/bridge_cpp/include/baml/version.h",
            f'inline constexpr const char* kToolchainVersion = "{version}";\n'
            f'inline constexpr const char* kBridgeRuntimeVersion = "{version}";\n',
        )
        for path in (
            "baml_language/sdks/typescript/bridge_typescript/src/version.rs",
            "baml_language/sdks/typescript/bridge_typescript_web/src/version.rs",
        ):
            self.write(
                path,
                f'pub(crate) const TOOLCHAIN_VERSION: &str = "{version}";\n'
                f'pub(crate) const BRIDGE_RUNTIME_VERSION: &str = "{version}";\n',
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
                r'^const ToolchainVersion = "([^"]+)"$',
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
                "go": "v2.3.5",
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
        self.assertEqual(plan["registry_versions"]["go"], f"v{canonical}")

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

    def test_replace_json_version_preserves_optional_comma(self) -> None:
        package = self.write(
            "package.json",
            '{\n  "name": "fixture",\n  "version": "1.2.3",\n  "private": true\n}\n',
        )
        version_tool_module().replace_json_version(package, "2.3.4")
        self.assertEqual(
            package.read_text(encoding="utf-8"),
            '{\n  "name": "fixture",\n  "version": "2.3.4",\n  "private": true\n}\n',
        )

        package.write_text(
            '{\n  "name": "fixture",\n  "version": "1.2.3"\n}\n',
            encoding="utf-8",
        )
        version_tool_module().replace_json_version(package, "2.3.4")
        self.assertEqual(
            package.read_text(encoding="utf-8"),
            '{\n  "name": "fixture",\n  "version": "2.3.4"\n}\n',
        )

    def test_exact_named_field_checks_reject_loose_version_text(self) -> None:
        rust_version = (
            self.root
            / "baml_language/sdks/rust/bridge_rust/src/version.rs"
        )
        rust_version.write_text(
            'pub(crate) const TOOLCHAIN_VERSION: &str = "9.9.9";\n'
            'pub(crate) const BRIDGE_RUNTIME_VERSION: &str = "1.2.3";\n'
            'pub(crate) const DECOY: &str = "1.2.3";\n',
            encoding="utf-8",
        )
        result = self.run_tool("check", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("rust bridge toolchain version", result.stderr)

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

    def test_nightly_letter_advances_past_the_nights_published_tags(self) -> None:
        # `already_cut` used to sit on top of this; the letter scan is what it
        # was really made of, and it is the only thing keeping two cuts on one
        # night from colliding.
        module = version_tool_module()
        base = module.Version.parse("1.2.4")
        env = {key: value for key, value in self.env.items() if "NIGHTLY_LETTER" not in key}
        with unittest.mock.patch.dict(os.environ, env, clear=True):
            letter = module.next_nightly_letter
            self.assertEqual(letter(base, "20260723", []), "a")
            self.assertEqual(
                letter(base, "20260723", ["baml-language-1.2.4-nightly.20260723.a"]), "b"
            )
            # Gaps do not reset it, and other bases/nights are not this night's.
            self.assertEqual(
                letter(
                    base,
                    "20260723",
                    [
                        "baml-language-1.2.4-nightly.20260723.a",
                        "baml-language-1.2.4-nightly.20260723.c",
                        "baml-language-1.2.4-nightly.20260722.z",
                        "baml-language-9.9.9-nightly.20260723.z",
                        "baml-language-1.2.4",
                    ],
                ),
                "d",
            )

    def test_nightly_refuses_to_plan_below_a_published_night(self) -> None:
        # The registries mostly resolve by highest version, but the nightly npm
        # dist-tag and the channel manifest are last-writer-wins, so a version
        # below the live one walks consumers backwards.
        module = version_tool_module()
        base = module.Version.parse("1.2.4")
        newest = module.newest_nightly_stamp
        self.assertIsNone(newest(base, ["baml-language-1.2.4"]))
        self.assertEqual(
            newest(
                base,
                [
                    "baml-language-1.2.4-nightly.20260722.b",
                    "baml-language-1.2.4-nightly.20260723.a",
                    "baml-language-9.9.9-nightly.20260830.a",
                ],
            ),
            ("20260723", "a"),
        )
        # The comparison the planner makes: night first, then letter.
        self.assertLess(("20260723", "a"), ("20260723", "b"))
        self.assertLess(("20260722", "z"), ("20260723", "a"))

    def test_tag_lookup_failure_is_loud_rather_than_an_empty_list(self) -> None:
        # Failing open here restarts lettering at `a` and re-mints a published
        # version, whose release assets the toolchain publisher then clobbers.
        stub = self.write("bin/gh", "#!/bin/sh\nexit 1\n")
        stub.chmod(0o755)
        stub = self.write("bin/git", "#!/bin/sh\nexit 1\n")
        stub.chmod(0o755)
        env = {
            key: value
            for key, value in self.env.items()
            if "NIGHTLY_LETTER" not in key and "VERSION_DATE" not in key
        }
        env["PATH"] = f"{self.bin}:{Path(sys.executable).parent}:/usr/bin:/bin"
        result = subprocess.run(
            [str(VERSION_TOOL), "compute", "--channel", "nightly"],
            cwd=self.root,
            env=env,
            check=False,
            text=True,
            capture_output=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("could not list release tags", result.stderr)

    def test_nightly_date_override_must_be_a_real_ascii_date(self) -> None:
        for value in ("20261345", "99999999", "٢٠٢٦٠٧٣٠", "2026073"):
            env = dict(self.env)
            env["BAML_LANGUAGE_VERSION_DATE"] = value
            result = subprocess.run(
                [str(VERSION_TOOL), "compute", "--channel", "nightly"],
                cwd=self.root,
                env=env,
                check=False,
                text=True,
                capture_output=True,
            )
            self.assertNotEqual(result.returncode, 0, value)
            self.assertIn("BAML_LANGUAGE_VERSION_DATE", result.stderr, value)


class NightNamingTests(unittest.TestCase):
    """The rule that decides which day a nightly cut is named for.

    Every nightly is a midnight cut, so it closes out the day that just ended.
    Deriving that from the local *date* rather than the local time makes the
    answer independent of how late in the slot the cut actually runs.
    """

    @staticmethod
    def night(utc: str) -> str:
        return version_tool_module().night_for(dt.datetime.fromisoformat(utc))

    def test_midnight_cut_closes_out_the_day_that_just_ended(self) -> None:
        # 07:00 UTC is midnight in Seattle while PDT, 08:00 UTC while PST; the
        # cron entry that is not local midnight never reaches this rule.
        self.assertEqual(self.night("2026-07-30T07:00:00+00:00"), "20260729")
        self.assertEqual(self.night("2026-01-15T08:00:00+00:00"), "20260114")

    def test_a_late_cut_still_names_the_night_it_was_scheduled_for(self) -> None:
        # The gate reads the clock in the first step and the release names the
        # night in a later job, so those two reads can straddle 01:00. Naming by
        # local date means they cannot disagree: only crossing the *next* local
        # midnight would change the answer.
        for utc in (
            "2026-07-30T07:59:00+00:00",  # 00:59 local, still in the slot
            "2026-07-30T09:30:00+00:00",  # 02:30 local, well past it
            "2026-07-30T22:00:00+00:00",  # 15:00 local, a manual repair run
        ):
            self.assertEqual(self.night(utc), "20260729", utc)

    def test_dst_transitions_keep_one_night_per_day(self) -> None:
        # Spring forward: 02:00 PST becomes 03:00 PDT on 2026-03-08, so the
        # 08:00 UTC entry is that night's local midnight.
        self.assertEqual(self.night("2026-03-08T08:00:00+00:00"), "20260307")
        # Fall back: 02:00 PDT becomes 01:00 PST on 2026-11-01, so the 07:00 UTC
        # entry is local midnight.
        self.assertEqual(self.night("2026-11-01T07:00:00+00:00"), "20261031")

    def test_a_naive_moment_is_rejected_rather_than_read_as_machine_local(self) -> None:
        with self.assertRaises(SystemExit):
            version_tool_module().night_for(dt.datetime(2026, 7, 30, 0, 30))

    def test_every_night_of_the_next_five_years_is_named_exactly_once(self) -> None:
        seattle = zoneinfo.ZoneInfo("America/Los_Angeles")
        day = dt.date(2026, 1, 1)
        named: list[str] = []
        while day < dt.date(2031, 1, 1):
            midnight_slots = [
                dt.datetime(day.year, day.month, day.day, hour, tzinfo=dt.timezone.utc)
                for hour in (7, 8)
                if dt.datetime(
                    day.year, day.month, day.day, hour, tzinfo=dt.timezone.utc
                )
                .astimezone(seattle)
                .hour
                == 0
            ]
            # Exactly one cron entry lands in the midnight hour, every day,
            # including the two DST transition days.
            self.assertEqual(len(midnight_slots), 1, day)
            named.append(self.night(midnight_slots[0].isoformat()))
            day += dt.timedelta(days=1)
        self.assertEqual(named, sorted(named))
        self.assertEqual(len(named), len(set(named)))


if __name__ == "__main__":
    unittest.main()
