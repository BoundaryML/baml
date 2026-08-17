#!/usr/bin/env python3
"""Cross-reference the vendor-profile crate set against the largest public
cargo-vet audit aggregation (https://github.com/google/rust-crate-audits).

When vendoring, a crate whose exact version is covered by a published audit
(directly or via a full-audit + delta-audit chain) is likely importable with
low friction; a crate with no published audit is the real review burden.

Usage (from engine/):
    cargo tree -p baml-python-ffi --no-default-features -e normal --prefix none \
      | sed -E 's/ \\(\\*\\)//; s/ \\(proc-macro\\)//' \
      | grep -v "$(pwd)" | awk '{print $1" "$2}' | sort -u > /tmp/vendor-crates.txt
    curl -sL -o /tmp/google-audits.toml \
      https://raw.githubusercontent.com/google/rust-crate-audits/main/audits.toml
    python3 scripts/vendor-audit-coverage.py /tmp/vendor-crates.txt /tmp/google-audits.toml
"""

import collections
import sys
import tomllib


def main(crates_path: str, audits_path: str) -> None:
    with open(audits_path, "rb") as f:
        data = tomllib.load(f)

    audits = data.get("audits", {})
    trusted = data.get("trusted", {})

    full = collections.defaultdict(set)
    deltas = collections.defaultdict(list)
    for crate, entries in audits.items():
        for e in entries:
            if "version" in e:
                full[crate].add(e["version"])
            elif "delta" in e:
                a, b = [x.strip() for x in e["delta"].split("->")]
                deltas[crate].append((a, b))

    def reachable(crate: str, version: str) -> bool:
        seen = set(full[crate])
        changed = True
        while changed and version not in seen:
            changed = False
            for a, b in deltas[crate]:
                if a in seen and b not in seen:
                    seen.add(b)
                    changed = True
        return version in seen

    ours = set()
    for line in open(crates_path):
        parts = line.split()
        if len(parts) == 2 and parts[1].startswith("v"):
            ours.add((parts[0], parts[1][1:]))

    exact, trusted_only, older, none = [], [], [], []
    for name, ver in sorted(ours):
        if name in audits and reachable(name, ver):
            exact.append((name, ver))
        elif name in trusted:
            trusted_only.append((name, ver))
        elif name in audits:
            have = sorted(full[name] | {b for _, b in deltas[name]})
            older.append((name, ver, have[-1] if have else "?"))
        else:
            none.append((name, ver))

    total = len(ours)
    print(f"Vendor profile crates: {total}")
    print(f"  Exact version has published audit: {len(exact):4d} ({100 * len(exact) // total}%)")
    print(f"  Trusted publisher (any version):  {len(trusted_only):4d}")
    print(f"  Audited, but different version:   {len(older):4d}")
    print(f"  No published audit:               {len(none):4d}")
    print("\n=== No published audit (the real review burden) ===")
    for n, v in none:
        print(f"  {n} v{v}")
    print("\n=== Version mismatch (latest audited version shown) ===")
    for n, v, latest in older:
        print(f"  {n}: ours v{v}, audited up to {latest}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(1)
    main(sys.argv[1], sys.argv[2])
