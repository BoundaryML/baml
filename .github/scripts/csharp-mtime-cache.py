#!/usr/bin/env python3
"""Content-aware mtime backdating for the C# sdk-test MSBuild cache.

MSBuild's incremental build is timestamp-based: a target is up to date when
its outputs are newer than its inputs. Caching `bin/`+`obj/` across CI runs
is therefore not enough on its own — a fresh checkout (and the build.rs
regeneration of the generated clients) stamps every input with the current
time, which makes every cached output look stale and forces the full ~20s
Roslyn rebuild per fixture that the cache was meant to avoid.

This script closes that gap by content, not by time:

- `record` walks the C# source roots (everything except `bin/`/`obj/`) and
  writes a `{path: sha256}` manifest into the cache directory.
- `restore` re-hashes the same tree and backdates ONLY byte-identical files
  to a fixed timestamp in 2000, i.e. older than any cached output. Files
  that changed — or are new, or of a kind we didn't anticipate — keep their
  fresh mtimes and rebuild exactly as they do today.

A missed input therefore fails toward a redundant rebuild, never toward a
stale test binary. With unchanged sources this restores MSBuild's no-op
path (~1s per fixture instead of ~20s, measured on macOS and mirrored by
the CI step times).
"""

import hashlib
import json
import os
import sys

# Everything the C# projects compile lives under these two trees: the bridge
# library + tools, and the fixture programs with their generated clients.
ROOTS = [
    "baml_language/sdks/csharp",
    "baml_language/sdk_tests/crates/csharp",
]
# bin/obj are build outputs (cached wholesale, never hashed); .baml is BAML's
# own runtime state (profile CAS + locks written during test execution) —
# volatile, content-addressed, and never an MSBuild input.
EXCLUDED_DIR_NAMES = {"bin", "obj", ".baml"}
MANIFEST = os.path.expanduser("~/.baml-csharp-ci/manifest.json")
# 2000-01-01 UTC: comfortably older than any output restored from the cache.
BACKDATED_MTIME = 946_684_800


def tracked_files():
    for root in ROOTS:
        for dirpath, dirnames, filenames in os.walk(root):
            dirnames[:] = sorted(d for d in dirnames if d not in EXCLUDED_DIR_NAMES)
            for name in sorted(filenames):
                yield os.path.join(dirpath, name).replace(os.sep, "/")


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def record():
    manifest = {path: digest(path) for path in tracked_files()}
    os.makedirs(os.path.dirname(MANIFEST), exist_ok=True)
    with open(MANIFEST, "w") as f:
        json.dump(manifest, f, indent=0)
    print(f"recorded {len(manifest)} files")


def restore():
    if not os.path.exists(MANIFEST):
        print("no manifest from a previous run; leaving all mtimes fresh")
        return
    with open(MANIFEST) as f:
        manifest = json.load(f)
    backdated = 0
    changed = []
    missing = []
    for path, want in manifest.items():
        try:
            if digest(path) == want:
                os.utime(path, (BACKDATED_MTIME, BACKDATED_MTIME))
                backdated += 1
            else:
                changed.append(path)
        except OSError:
            # Deleted or unreadable file: nothing to backdate; any project
            # that referenced it rebuilds via its own fresh inputs.
            missing.append(path)
    new = [p for p in tracked_files() if p not in manifest]
    print(f"backdated {backdated}/{len(manifest)} unchanged files")
    for label, paths in (("changed", changed), ("missing", missing), ("new", new)):
        if not paths:
            continue
        by_prefix = {}
        for p in paths:
            prefix = "/".join(p.split("/")[:6])
            by_prefix[prefix] = by_prefix.get(prefix, 0) + 1
        print(f"{label}: {len(paths)} files")
        for prefix, count in sorted(by_prefix.items(), key=lambda kv: -kv[1])[:15]:
            print(f"  {count:5d}  {prefix}")
        for p in paths[:8]:
            print(f"    e.g. {p}")


def main():
    actions = {"record": record, "restore": restore}
    if len(sys.argv) != 2 or sys.argv[1] not in actions:
        sys.exit(f"usage: {sys.argv[0]} {{record|restore}}")
    actions[sys.argv[1]]()


if __name__ == "__main__":
    main()
