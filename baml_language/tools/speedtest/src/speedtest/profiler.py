"""Samply profiling + profile diff (symbolication via atos)."""

import gzip
import json
import os
import re
import subprocess
import sys
from collections import Counter


# ── Profile loading ──────────────────────────────────────────────────────

def load_profile(path):
    opener = gzip.open if path.endswith(".gz") else open
    with opener(path, "rt") as f:
        return json.load(f)


def _resolve_string(strings, idx):
    if idx is None or idx < 0:
        return "<none>"
    return strings[idx]


def _find_lib_idx(profile, binary_path):
    basename = os.path.basename(binary_path)
    for i, lib in enumerate(profile["libs"]):
        if lib.get("name") == basename:
            return i
    return None


# ── Symbolication (expensive — once per binary) ─────────────────────────

def _collect_addresses(profile, binary_path):
    lib_idx = _find_lib_idx(profile, binary_path)
    if lib_idx is None:
        return set()
    addrs = set()
    for thread in profile["threads"]:
        ft = thread["funcTable"]
        rt = thread["resourceTable"]
        sa = thread["stringArray"]
        for i in range(ft["length"]):
            res_idx = ft["resource"][i]
            if res_idx < 0 or res_idx >= rt["length"]:
                continue
            if rt["lib"][res_idx] != lib_idx:
                continue
            name_str = sa[ft["name"][i]]
            if name_str.startswith("0x"):
                try:
                    addrs.add(int(name_str, 16))
                except ValueError:
                    pass
    return addrs


def _resolve_addresses(binary_path, addresses):
    if not addresses:
        return {}
    uniq = sorted(addresses)
    try:
        result = subprocess.run(
            ["atos", "-o", binary_path, "-l", "0x100000000"]
            + [f"0x{a + 0x100000000:x}" for a in uniq],
            capture_output=True, text=True,
        )
        lines = result.stdout.strip().split("\n")
        return {
            addr: (line.strip() if line.strip() else f"0x{addr:x}")
            for addr, line in zip(uniq, lines)
        }
    except Exception as e:
        print(f"atos failed: {e}", file=sys.stderr)
        return {a: f"0x{a:x}" for a in uniq}


def _symbolicate_binary(binary_path, profiles):
    all_addrs = set()
    for p in profiles:
        all_addrs |= _collect_addresses(p, binary_path)
    if not all_addrs:
        return {}
    print(f"  Symbolicating {len(all_addrs)} addresses from {binary_path}", file=sys.stderr)
    return _resolve_addresses(binary_path, all_addrs)


# ── Per-profile name map ────────────────────────────────────────────────

def _build_name_map(profile, binary_path, addr_cache):
    lib_idx = _find_lib_idx(profile, binary_path)
    name_map = {}
    for thread_idx, thread in enumerate(profile["threads"]):
        ft = thread["funcTable"]
        rt = thread["resourceTable"]
        sa = thread["stringArray"]
        for i in range(ft["length"]):
            res_idx = ft["resource"][i]
            if res_idx < 0 or res_idx >= rt["length"]:
                continue
            if rt["lib"][res_idx] != lib_idx:
                continue
            name_str = sa[ft["name"][i]]
            if name_str.startswith("0x"):
                try:
                    addr = int(name_str, 16)
                    if addr in addr_cache:
                        name_map[(thread_idx, i)] = addr_cache[addr]
                except ValueError:
                    pass
    return name_map


# ── Sample counting ──────────────────────────────────────────────────────

def _normalize(name):
    name = re.sub(r"::h[0-9a-f]+\b", "", name)
    name = re.sub(r"\s*\([^)]+\)\s*$", "", name)
    return name.strip()


def _shorten(name, maxlen=110):
    return name if len(name) <= maxlen else name[:maxlen - 3] + "..."


def _stack_frames(thread, stack_idx):
    st = thread["stackTable"]
    prefix_col, frame_col = st["prefix"], st["frame"]
    frames = []
    while stack_idx is not None and stack_idx >= 0:
        frames.append(frame_col[stack_idx])
        stack_idx = prefix_col[stack_idx]
    frames.reverse()
    return frames


def _count_samples(profile, name_map):
    leaf = Counter()
    inclusive = Counter()
    for thread_idx, thread in enumerate(profile["threads"]):
        ft = thread["funcTable"]
        sa = thread["stringArray"]

        def func_name(f_idx):
            func_idx = thread["frameTable"]["func"][f_idx]
            sym = name_map.get((thread_idx, func_idx))
            if sym is None:
                sym = _resolve_string(sa, ft["name"][func_idx])
            return _normalize(sym)

        for stack_idx in thread["samples"]["stack"]:
            if stack_idx is None or stack_idx < 0:
                continue
            frames = _stack_frames(thread, stack_idx)
            names = [func_name(f) for f in frames]
            seen = set()
            for n in names:
                if n not in seen:
                    inclusive[n] += 1
                    seen.add(n)
            if names:
                leaf[names[-1]] += 1
    return leaf, inclusive


# ── Diff display ─────────────────────────────────────────────────────────

def _print_diff(name, base_leaf, base_total, curr_leaf, curr_total, top_n=10, out=None):
    if out is None:
        out = sys.stdout

    print(f"\n{'='*120}", file=out)
    print(f"  PROFILE DIFF: {name}", file=out)
    print(f"{'='*120}", file=out)
    print(f"\nBaseline: ({base_total} samples)  Current: ({curr_total} samples)", file=out)

    print(f"\n--- BASELINE top leaf ---", file=out)
    print(f"{'samples':>10}  {'%':>6}  function", file=out)
    for n, c in base_leaf.most_common(top_n):
        print(f"{c:>10}  {c/base_total*100:>5.1f}%  {_shorten(n)}", file=out)

    print(f"\n--- CURRENT top leaf ---", file=out)
    print(f"{'samples':>10}  {'%':>6}  function", file=out)
    for n, c in curr_leaf.most_common(top_n):
        print(f"{c:>10}  {c/curr_total*100:>5.1f}%  {_shorten(n)}", file=out)

    all_funcs = set(base_leaf.keys()) | set(curr_leaf.keys())
    diffs = []
    for n in all_funcs:
        bp = base_leaf.get(n, 0) / base_total * 100
        cp = curr_leaf.get(n, 0) / curr_total * 100
        delta = cp - bp
        if max(bp, cp) >= 0.5:
            diffs.append((delta, n, bp, cp))
    diffs.sort(key=lambda x: x[0])

    print(f"\n--- Functions COLDER in current (less CPU) ---", file=out)
    print(f"{'delta':>8}  {'base%':>7}  {'curr%':>7}  function", file=out)
    for delta, n, bp, cp in diffs[:top_n]:
        if delta < -0.3:
            print(f"{delta:>+8.2f}  {bp:>6.2f}%  {cp:>6.2f}%  {_shorten(n)}", file=out)

    print(f"\n--- Functions HOTTER in current (more CPU) ---", file=out)
    print(f"{'delta':>8}  {'base%':>7}  {'curr%':>7}  function", file=out)
    for delta, n, bp, cp in reversed(diffs[-top_n:]):
        if delta > 0.3:
            print(f"{delta:>+8.2f}  {bp:>6.2f}%  {cp:>6.2f}%  {_shorten(n)}", file=out)


# ── Public batch API ─────────────────────────────────────────────────────

def diff_profiles_batch(pairs, base_bin, curr_bin, top_n=10, out=None):
    """Diff multiple profile pairs with shared symbolication (2 atos calls total)."""
    if out is None:
        out = sys.stdout

    print(f"  Loading {len(pairs)} profile pairs...", file=sys.stderr)
    loaded = []
    for name, bp, cp in pairs:
        loaded.append((name, load_profile(bp), load_profile(cp)))

    base_cache = _symbolicate_binary(base_bin, [b for _, b, _ in loaded])
    curr_cache = _symbolicate_binary(curr_bin, [c for _, _, c in loaded])

    for name, base_prof, curr_prof in loaded:
        base_names = _build_name_map(base_prof, base_bin, base_cache)
        curr_names = _build_name_map(curr_prof, curr_bin, curr_cache)
        base_leaf, _ = _count_samples(base_prof, base_names)
        curr_leaf, _ = _count_samples(curr_prof, curr_names)
        base_total = sum(base_leaf.values()) or 1
        curr_total = sum(curr_leaf.values()) or 1
        _print_diff(name, base_leaf, base_total, curr_leaf, curr_total, top_n=top_n, out=out)
