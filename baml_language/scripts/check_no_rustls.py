#!/usr/bin/env python3
"""Reject rustls packages in selected artifacts' normal and build dependencies."""

import argparse
import subprocess
import sys
from pathlib import Path


def assert_no_rustls(names):
    forbidden = sorted(name for name in names if name.startswith("rustls") or name.endswith("-rustls"))
    if forbidden:
        raise RuntimeError(f"rustls dependencies found: {', '.join(forbidden)}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("-p", "--package", action="append", required=True)
    parser.add_argument("--target")
    parser.add_argument("--features", action="append", default=[])
    parser.add_argument("--no-default-features", action="store_true")
    parser.add_argument("--all-features", action="store_true")
    options = parser.parse_args(sys.argv[2:] if sys.argv[1:2] == ["--"] else None)
    args = []
    for package in options.package:
        args += ["-p", package]
    if options.target:
        args += ["--target", options.target]
    for features in options.features:
        args += ["--features", features]
    if options.no_default_features:
        args += ["--no-default-features"]
    if options.all_features:
        args += ["--all-features"]
    result = subprocess.run(
        ["cargo", "tree", "--locked", *args, "-e", "normal,build", "--prefix", "none", "--format", "{p}"],
        cwd=Path(__file__).resolve().parent.parent, check=True, text=True, stdout=subprocess.PIPE,
    )
    names = {line.split()[0] for line in result.stdout.splitlines() if line.strip()}
    if not names:
        raise RuntimeError("Cargo returned an empty dependency graph")
    assert_no_rustls(names)
    print(f"Verified {len(names)} packages: no rustls dependencies (including build dependencies).")


if __name__ == "__main__":
    main()
