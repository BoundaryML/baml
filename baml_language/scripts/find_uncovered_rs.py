#!/usr/bin/env python3
"""Find *.rs files under the baml_language tree that are NOT covered by the
current Cargo workspace.

"Covered" is defined by Cargo's own view of the world:

  1. `cargo metadata --no-deps` gives the directories of every workspace
     *member* package (the dir holding each member's Cargo.toml).
  2. Every *.rs file is attributed to its nearest ancestor Cargo.toml.
  3. A file is covered iff that nearest manifest belongs to a member package.

This correctly handles:
  - `exclude = [...]` crates and other non-member crates (their files are
    reported as uncovered).
  - nested fixture/test crates inside a member dir (the nested Cargo.toml
    shadows the enclosing member, so those files are uncovered).
  - stray *.rs files with no ancestor Cargo.toml at all.

Build/target artifacts are skipped. Exit status is non-zero when any
uncovered files are found, so this is usable as a CI guard.

Usage:
    scripts/find_uncovered_rs.py [WORKSPACE_DIR]   # default: cargo workspace
                                                    # containing the cwd
"""

from __future__ import annotations

import json
import os
import subprocess
import sys

# Directory names we never descend into while scanning for *.rs / Cargo.toml.
PRUNE_DIRS = {
    "target",
    "target-rust-analyzer",
    "node_modules",
    ".git",
    ".jj",
}


def cargo_member_dirs(workspace_dir: str) -> tuple[str, set[str]]:
    """Return (workspace_root, {member package dirs}) via `cargo metadata`."""
    out = subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=workspace_dir,
        text=True,
    )
    meta = json.loads(out)
    member_ids = set(meta["workspace_members"])
    member_dirs = {
        os.path.dirname(pkg["manifest_path"])
        for pkg in meta["packages"]
        if pkg["id"] in member_ids
    }
    return os.path.realpath(meta["workspace_root"]), member_dirs


def scan(root: str) -> tuple[list[str], set[str]]:
    """Walk `root`, returning (all *.rs files, dirs that contain a Cargo.toml)."""
    rs_files: list[str] = []
    manifest_dirs: set[str] = set()
    for dirpath, dirnames, filenames in os.walk(root):
        # Prune noisy / generated trees in place so we never descend into them.
        dirnames[:] = [d for d in dirnames if d not in PRUNE_DIRS]
        if "Cargo.toml" in filenames:
            manifest_dirs.add(os.path.realpath(dirpath))
        for name in filenames:
            if name.endswith(".rs"):
                rs_files.append(os.path.realpath(os.path.join(dirpath, name)))
    return rs_files, manifest_dirs


def nearest_manifest_dir(file_path: str, manifest_dirs: set[str], root: str) -> str | None:
    """Nearest ancestor dir of `file_path` that holds a Cargo.toml, or None."""
    d = os.path.dirname(file_path)
    while True:
        if d in manifest_dirs:
            return d
        if d == root or d == os.path.dirname(d):  # hit workspace root or fs root
            return None
        d = os.path.dirname(d)


def main() -> int:
    workspace_dir = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else os.getcwd()

    root, member_dirs = cargo_member_dirs(workspace_dir)
    rs_files, manifest_dirs = scan(root)

    # group uncovered files by the manifest that "owns" them (or "<no manifest>")
    uncovered: dict[str, list[str]] = {}
    for f in rs_files:
        owner = nearest_manifest_dir(f, manifest_dirs, root)
        if owner in member_dirs:
            continue
        key = owner if owner is not None else "<no Cargo.toml ancestor>"
        uncovered.setdefault(key, []).append(f)

    total = sum(len(v) for v in uncovered.values())
    if total == 0:
        print(f"All {len(rs_files)} *.rs files are covered by the workspace.")
        return 0

    print(
        f"{total} of {len(rs_files)} *.rs files are NOT covered by the workspace "
        f"({len(uncovered)} owning location(s)):\n"
    )
    for owner in sorted(uncovered):
        label = owner if owner == "<no Cargo.toml ancestor>" else os.path.relpath(owner, root)
        files = sorted(uncovered[owner])
        print(f"  {label}  ({len(files)} file(s))")
        for f in files:
            print(f"      {os.path.relpath(f, root)}")
        print()

    return 1


if __name__ == "__main__":
    sys.exit(main())
