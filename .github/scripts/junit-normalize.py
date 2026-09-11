#!/usr/bin/env python3
"""Add source-file paths to JUnit reports whose runner cannot emit them.

File is used for codeowners and for flaky test fix investigations, so ensure
the junit is enriched when it can be. See docs/flaky-tests-uploads.md.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import xml.etree.ElementTree as ET
from collections import OrderedDict
from datetime import datetime, timezone
from pathlib import Path


def target_source_map(manifest_path: Path) -> dict[tuple[str, str, str], Path]:
    raw = subprocess.run(
        [
            "cargo",
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
            str(manifest_path),
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout

    sources: dict[tuple[str, str, str], Path] = {}
    for package in json.loads(raw)["packages"]:
        for target in package["targets"]:
            for kind in target["kind"]:
                sources[(package["name"], target["name"], kind)] = Path(
                    target["src_path"]
                )
    return sources


def resolve_binary_id(
    binary_id: str, sources: dict[tuple[str, str, str], Path]
) -> Path | None:
    package, separator, remainder = binary_id.partition("::")
    if not separator:
        return sources.get((package, package, "lib"))
    if remainder.startswith("bin/"):
        return sources.get((package, remainder[len("bin/") :], "bin"))
    return sources.get((package, remainder, "test"))


def normalize_nextest(report: Path, args) -> int:
    """Stamp each testsuite's binary source file onto it and its cases."""
    sources = target_source_map(args.manifest_path.resolve())
    repo_root = args.repo_root.resolve()
    tree = ET.parse(report)
    annotated = 0
    unresolved: list[str] = []

    for suite in tree.getroot().iter("testsuite"):
        binary_id = suite.get("name")
        if not binary_id:
            continue
        source = resolve_binary_id(binary_id, sources)
        if source is None:
            unresolved.append(binary_id)
            continue
        try:
            relative = source.relative_to(repo_root).as_posix()
        except ValueError:
            unresolved.append(binary_id)
            continue
        for case in suite.iter("testcase"):
            case.set("file", relative)
            annotated += 1

    tree.write(report, encoding="utf-8", xml_declaration=True)
    print(f"stamped file paths onto {annotated} test cases in {report}")
    for binary_id in unresolved:
        print(
            f"::warning::no cargo target matches testsuite '{binary_id}'; "
            "its tests will upload without a file path",
            file=sys.stderr,
        )
    return 1 if unresolved and args.strict else 0


def normalize_node_test(report: Path, args) -> int:
    """Group orphan testcases into one suite per file, with a timestamp."""
    repo_root = args.repo_root.resolve()
    root = ET.parse(report).getroot()
    orphans = [case for case in list(root) if case.tag == "testcase"]
    if not orphans:
        print(f"::warning::no ungrouped test cases in {report}", file=sys.stderr)
        return 0

    finished = (
        datetime.fromtimestamp(report.stat().st_mtime, tz=timezone.utc)
        .isoformat(timespec="milliseconds")
        .replace("+00:00", "Z")
    )

    by_file: OrderedDict[str, list[ET.Element]] = OrderedDict()
    for case in orphans:
        raw = Path(case.get("file", "unknown"))
        if raw.is_absolute():
            try:
                source = raw.resolve().relative_to(repo_root).as_posix()
            except ValueError:
                source = raw.as_posix()
        else:
            source = raw.as_posix()
        case.set("file", source)
        case.set("classname", source)
        by_file.setdefault(source, []).append(case)
        root.remove(case)

    for source, cases in by_file.items():
        suite = ET.SubElement(
            root,
            "testsuite",
            {
                "name": source,
                "file": source,
                "tests": str(len(cases)),
                "failures": str(
                    sum(1 for c in cases if c.find("failure") is not None)
                ),
                "errors": "0",
                "skipped": str(
                    sum(1 for c in cases if c.find("skipped") is not None)
                ),
                "time": f"{sum(float(c.get('time') or 0) for c in cases):.6f}",
                "timestamp": finished,
            },
        )
        suite.extend(cases)

    ET.ElementTree(root).write(report, encoding="utf-8", xml_declaration=True)
    print(f"grouped {len(orphans)} cases into {len(by_file)} suites in {report}")
    return 0


MODES = {
    "nextest": normalize_nextest,
    "node-test": normalize_node_test,
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=sorted(MODES))
    parser.add_argument("report", type=Path, help="JUnit report to rewrite in place")
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path.cwd(),
        help="paths are written relative to this (default: cwd)",
    )
    parser.add_argument(
        "--manifest-path",
        type=Path,
        help="nextest mode: Cargo.toml the report came from",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="nextest mode: fail instead of warning on an unresolved testsuite",
    )
    args = parser.parse_args()

    if not args.report.is_file():
        print(f"::error::no JUnit report at {args.report}", file=sys.stderr)
        return 1
    if args.mode == "nextest" and not args.manifest_path:
        print("::error::nextest mode needs --manifest-path", file=sys.stderr)
        return 1

    return MODES[args.mode](args.report, args)


if __name__ == "__main__":
    sys.exit(main())
