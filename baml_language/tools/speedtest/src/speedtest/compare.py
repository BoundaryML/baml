"""Compare subcommand — critcmp-style terminal diff of two runs."""

import json as json_mod
import sys

from speedtest.fmt import (
    BOLD, DIM, GREEN, RED, RESET, color_delta, fmt_delta_pct, fmt_ms,
    sig_marker,
)
from speedtest.storage import resolve_ref, results_dir


def _cli_vcs(run_data):
    cli = run_data.get("cli", {}) or {}
    return cli.get("vcs") or cli.get("git") or {}


def cmd_compare(args):
    rdir = results_dir(args)
    a = resolve_ref(rdir, args.a)
    b = resolve_ref(rdir, args.b)

    if not a:
        print(f"ERROR: '{args.a}' not found in {rdir}", file=sys.stderr)
        sys.exit(1)
    if not b:
        print(f"ERROR: '{args.b}' not found in {rdir}", file=sys.stderr)
        sys.exit(1)

    return _render_md(args, a, b)
    a_vcs = _cli_vcs(a)
    b_vcs = _cli_vcs(b)
    a_commit = a_vcs.get("commit", "?")[:7]
    b_commit = b_vcs.get("commit", "?")[:7]
    a_label = args.a
    b_label = args.b

    # Truncate labels for display
    max_label = 20
    a_disp = a_label[:max_label] if len(a_label) > max_label else a_label
    b_disp = b_label[:max_label] if len(b_label) > max_label else b_label

    b_by_name = {w["name"]: w for w in b.get("workloads", [])}

    print()
    print(f"  {BOLD}{a_disp}{RESET} ({a_commit}) → {BOLD}{b_disp}{RESET} ({b_commit})")
    if a_vcs.get("message"):
        print(f"  before: {DIM}{a_vcs['message'][:60]}{RESET}")
    if b_vcs.get("message"):
        print(f"  after:  {DIM}{b_vcs['message'][:60]}{RESET}")
    print()

    NAME_W = 30
    NUM_W = 12

    # Group by category
    categories = {}
    for w in a.get("workloads", []):
        cat = w.get("category", "other")
        if cat not in categories:
            categories[cat] = []
        categories[cat].append(w)
    # Add workloads only in B
    for w in b.get("workloads", []):
        cat = w.get("category", "other")
        if cat not in categories:
            categories[cat] = []
        if not any(x["name"] == w["name"] for x in categories[cat]):
            categories[cat].append(w)

    total_faster = total_slower = total_unchanged = 0

    for cat, workloads in categories.items():
        if args.filter and not any(f.lower() in cat.lower() for f in args.filter):
            # Check if any workload matches
            if not any(any(f.lower() in w["name"].lower() for f in args.filter) for w in workloads):
                continue

        print(f"  {DIM}── {cat.upper()} ──{RESET}")
        print(f"  {'workload':<{NAME_W}s} {a_disp:>{NUM_W}s} {'→':>3s} {b_disp:>{NUM_W}s} {'change':>{NUM_W}s}")
        print(f"  {'-'*NAME_W} {'-'*NUM_W} {'':>3s} {'-'*NUM_W} {'-'*NUM_W}")

        cat_faster = cat_slower = cat_unchanged = 0

        for wA in workloads:
            name = wA["name"]
            if args.filter and not any(f.lower() in name.lower() for f in args.filter):
                continue

            wB = b_by_name.get(name)
            display = name

            a_baml = (wA.get("results") or {}).get("baml")
            b_baml = (wB.get("results") or {}).get("baml") if wB else None

            a_med = a_baml["med"] if a_baml else None
            b_med = b_baml["med"] if b_baml else None
            a_sd = a_baml.get("sd", 0) if a_baml else 0
            b_sd = b_baml.get("sd", 0) if b_baml else 0

            a_str = fmt_ms(a_med) if a_med else "—"
            b_str = fmt_ms(b_med) if b_med else "—"
            delta = fmt_delta_pct(a_med, b_med)

            is_sig = sig_marker(b_med, b_sd, a_med, a_sd) if (a_med and b_med) else False
            pct_val = ((b_med - a_med) / a_med * 100) if (a_med and b_med and a_med > 0) else None
            abs_pct = abs(pct_val) if pct_val is not None else 0
            notable = is_sig or abs_pct > 5

            if args.threshold and abs_pct < args.threshold:
                cat_unchanged += 1
                continue

            marker = " *" if is_sig else " ~" if notable and not is_sig else ""
            delta_colored = color_delta(delta, notable)

            # Source changed indicator
            src_flag = ""
            if wB:
                a_src = wA.get("source", {}).get("baml", "")
                b_src = wB.get("source", {}).get("baml", "")
                if a_src and b_src and a_src != b_src:
                    src_flag = f" {DIM}(src changed){RESET}"

            print(f"  {display:<{NAME_W}s} {a_str:>{NUM_W}s}  →  {b_str:>{NUM_W}s} {delta_colored:>{NUM_W + 10}s}{marker}{src_flag}")

            if notable and pct_val is not None:
                if pct_val < 0:
                    cat_faster += 1
                else:
                    cat_slower += 1
            else:
                cat_unchanged += 1

        total_faster += cat_faster
        total_slower += cat_slower
        total_unchanged += cat_unchanged
        print()

    # Summary
    parts = []
    if total_faster:
        parts.append(f"{GREEN}{total_faster} faster{RESET}")
    if total_slower:
        parts.append(f"{RED}{total_slower} slower{RESET}")
    if total_unchanged:
        parts.append(f"{DIM}{total_unchanged} unchanged{RESET}")
    print(f"  {', '.join(parts)}")
    print()


def _render_md(args, a, b):
    """Render comparison as a markdown table."""
    a_vcs = _cli_vcs(a)
    b_vcs = _cli_vcs(b)
    a_commit = a_vcs.get("commit", "?")[:7]
    b_commit = b_vcs.get("commit", "?")[:7]

    b_by_name = {w["name"]: w for w in b.get("workloads", [])}

    # Group by category
    categories = {}
    for w in [*a.get("workloads", []), *b.get("workloads", [])]:
        cat = w.get("category", "other")
        if cat not in categories:
            categories[cat] = {}
        categories[cat][w["name"]] = None
    # Re-walk to get actual workload objects
    cat_workloads = {}
    for cat in categories:
        cat_workloads[cat] = []
        seen = set()
        for w in [*a.get("workloads", []), *b.get("workloads", [])]:
            if w.get("category") == cat and w["name"] not in seen:
                seen.add(w["name"])
                cat_workloads[cat].append(w)

    print(f"## speedtest: `{args.a}` ({a_commit}) → `{args.b}` ({b_commit})")
    if a_vcs.get("message"):
        print(f"> before: {a_vcs['message'][:80]}")
    if b_vcs.get("message"):
        print(f"> after: {b_vcs['message'][:80]}")
    print()

    faster = slower = unch = 0

    for cat, workloads in cat_workloads.items():
        if args.filter and not any(
            any(f.lower() in w["name"].lower() for f in args.filter) for w in workloads
        ):
            continue

        print(f"### {cat}")
        print()
        # Detect which extra runners are available in either run
        extra_runners = []
        for runner in ("python", "node", "bun"):
            has_it = any(
                (w.get("results") or {}).get(runner) is not None
                for w in [*a.get("workloads", []), *b.get("workloads", [])]
                if w.get("category") == cat
            )
            if has_it:
                extra_runners.append(runner)

        extra_hdrs = "".join(f" {r} | vs baml |" for r in extra_runners)
        extra_seps = "".join(" -------:| -------:|" for _ in extra_runners)
        print(f"| Workload | {args.a} | {args.b} | Change |{extra_hdrs}")
        print(f"|----------|-------:|-------:|-------:|{extra_seps}")

        for wA in workloads:
            name = wA["name"]
            if args.filter and not any(f.lower() in name.lower() for f in args.filter):
                continue
            wB = b_by_name.get(name)
            display = name

            a_baml = (wA.get("results") or {}).get("baml")
            b_baml = (wB.get("results") or {}).get("baml") if wB else None

            a_med = a_baml["med"] if a_baml else None
            b_med = b_baml["med"] if b_baml else None
            a_sd = a_baml.get("sd", 0) if a_baml else 0
            b_sd = b_baml.get("sd", 0) if b_baml else 0

            a_str = fmt_ms(a_med) if a_med else "—"
            b_str = fmt_ms(b_med) if b_med else "—"

            is_sig = sig_marker(b_med, b_sd, a_med, a_sd) if (a_med and b_med) else False
            pct_val = ((b_med - a_med) / a_med * 100) if (a_med and b_med and a_med > 0) else None
            abs_pct = abs(pct_val) if pct_val is not None else 0
            notable = is_sig or abs_pct > 5

            if pct_val is not None:
                sign = "+" if pct_val >= 0 else ""
                pct_str = f"{sign}{pct_val:.1f}%"
                if notable:
                    if pct_val < 0:
                        pct_str = f"**{pct_str}** :arrow_down:"
                        faster += 1
                    else:
                        pct_str = f"**{pct_str}** :arrow_up:"
                        slower += 1
                else:
                    unch += 1
            else:
                pct_str = "—"
                unch += 1

            if args.threshold and abs_pct < args.threshold:
                continue

            # Extra runner columns — use the "after" run's data, fall back to "before"
            extra_cells = ""
            for runner in extra_runners:
                r_data = (wB.get("results") or {}).get(runner) if wB else None
                if not r_data:
                    r_data = (wA.get("results") or {}).get(runner)
                r_ms = fmt_ms(r_data["med"]) if r_data else "—"
                # Ratio: baml(after) / runner
                baml_med = b_med if b_med else a_med
                r_med = r_data["med"] if r_data else None
                if baml_med and r_med and r_med > 0:
                    ratio = baml_med / r_med
                    ratio_str = f"{ratio:.1f}x" if ratio < 10 else f"{ratio:.0f}x"
                else:
                    ratio_str = "—"
                extra_cells += f" {r_ms} | {ratio_str} |"

            print(f"| {display} | {a_str} | {b_str} | {pct_str} |{extra_cells}")

        print()

    parts = []
    if faster:
        parts.append(f"**{faster} faster**")
    if slower:
        parts.append(f"**{slower} slower**")
    if unch:
        parts.append(f"{unch} unchanged")
    print(f"Summary: {', '.join(parts)}")
