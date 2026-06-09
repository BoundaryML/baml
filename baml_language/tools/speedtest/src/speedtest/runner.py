"""Benchmark runner — timing, packing, the main run loop."""

import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from statistics import median, stdev

from speedtest.fmt import fmt_ms, fmt_ms_sd, fmt_ratio, sig_marker
from speedtest.loader import load_workloads
from speedtest.storage import (
    build_run_data, results_dir,
    save_baseline, save_run,
)


def _time_once(cmd):
    """Run a command once, return (elapsed_seconds, success, result)."""
    start = time.perf_counter()
    result = subprocess.run(cmd, capture_output=True, text=True, shell=isinstance(cmd, str))
    elapsed = time.perf_counter() - start
    return elapsed, result.returncode == 0, result


def time_command_adaptive(cmd, measurement_time=5.0, min_samples=5, max_samples=100):
    """Run a command adaptively, targeting measurement_time seconds total.

    Strategy (criterion-inspired):
      1. Warm up: 3 runs (discarded, warms OS caches / CPU)
      2. Estimate per-run time from warm-up
      3. Calculate how many samples to fill measurement_time
      4. Clamp to [min_samples, max_samples]
      5. Collect samples

    Returns list of wall-clock seconds or None on failure.
    """
    warmup_times = []
    for i in range(3):
        elapsed, ok, result = _time_once(cmd)
        if not ok:
            if i == 0:
                stderr = result.stderr.strip()
                stderr = re.sub(r'\x1b\[[0-9;]*m', '', stderr)
                lines = stderr.split('\n')
                err_lines = [l for l in lines if 'error' in l.lower() or 'traceback' in l.lower()]
                if err_lines:
                    sys.stderr.write(f" ERROR: {err_lines[0][:80]}\n")
            return None
        warmup_times.append(elapsed)

    est_per_run = median(warmup_times)
    if est_per_run > 0:
        target_samples = int(measurement_time / est_per_run)
    else:
        target_samples = max_samples
    n_samples = max(min_samples, min(target_samples, max_samples))

    times = []
    for i in range(n_samples):
        elapsed, ok, result = _time_once(cmd)
        if not ok:
            return None
        times.append(elapsed)

    return times


def time_command_fixed(cmd, runs=10):
    """Run a command a fixed number of times. Returns list of seconds or None."""
    times = []
    for i in range(runs):
        elapsed, ok, result = _time_once(cmd)
        if not ok:
            if i == 0:
                stderr = result.stderr.strip()
                stderr = re.sub(r'\x1b\[[0-9;]*m', '', stderr)
                lines = stderr.split('\n')
                err_lines = [l for l in lines if 'error' in l.lower() or 'traceback' in l.lower()]
                if err_lines:
                    sys.stderr.write(f" ERROR: {err_lines[0][:80]}\n")
            return None
        times.append(elapsed)
    return times


def pack_baml(binary, baml_file, output_path):
    """Pack a BAML workload into a standalone binary. Returns True on success."""
    result = subprocess.run(
        [binary, "pack", "main", "--file", baml_file, "-o", output_path],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        sys.stderr.write(
            f"       baml pack failed (binary={binary}): "
            f"{result.stderr.strip().splitlines()[-1] if result.stderr.strip() else 'no stderr'}\n"
        )
        return False
    return True


def _repo_root():
    d = Path.cwd()
    while d != d.parent:
        if (d / "Cargo.toml").exists():
            return d
        d = d.parent
    return Path.cwd()


def cmd_run(args):
    """Execute the 'run' subcommand — the main benchmark loop."""
    repo_root = _repo_root()
    baml_cli = os.path.abspath(args.baml or str(repo_root / "target" / "release" / "baml-cli"))

    # ── Build ────────────────────────────────────────────────────────────
    if args.build:
        build_cmd = ["cargo", "build", "--release", "-p", "baml_pack_host", "-p", "baml_cli"]
        print(f"$ {' '.join(build_cmd)}", file=sys.stderr)
        r = subprocess.run(build_cmd, cwd=str(repo_root))
        if r.returncode != 0:
            sys.exit(r.returncode)
        if args.profile:
            prof_cmd = ["cargo", "build", "--profile", "profiling", "-p", "baml_pack_host", "-p", "baml_cli"]
            print(f"$ {' '.join(prof_cmd)}", file=sys.stderr)
            r = subprocess.run(prof_cmd, cwd=str(repo_root))
            if r.returncode != 0:
                sys.exit(r.returncode)

    if not os.path.isfile(baml_cli):
        print(f"ERROR: baml-cli not found at {baml_cli}")
        print("Run: cargo build --release --bin baml-cli  (or use --build)")
        sys.exit(1)

    # ── Detect runners ───────────────────────────────────────────────────
    if args.only_baml:
        has_python = has_bun = has_node = False
    else:
        has_python = subprocess.run(["python3", "--version"], capture_output=True).returncode == 0
        has_bun = subprocess.run(["bun", "--version"], capture_output=True).returncode == 0
        has_node = subprocess.run(["node", "--version"], capture_output=True).returncode == 0

    # ── Profiling setup ──────────────────────────────────────────────────
    profile_baml_bin = None
    if args.profile:
        if subprocess.run(["which", "samply"], capture_output=True).returncode != 0:
            print("ERROR: --profile requires `samply` on PATH.", file=sys.stderr)
            sys.exit(1)

        def derive_prof(p):
            return p.replace("/release/", "/profiling/")

        profile_baml_bin = args.profile_baml or derive_prof(baml_cli)
        if not os.path.isfile(profile_baml_bin):
            print(f"ERROR: profiling binary not found at {profile_baml_bin}", file=sys.stderr)
            sys.exit(1)

    # ── Load workloads ───────────────────────────────────────────────────
    workloads = load_workloads()
    if not workloads:
        print("ERROR: no workloads found", file=sys.stderr)
        sys.exit(1)

    if args.filter:
        workloads = [w for w in workloads if any(f.lower() in w["name"].lower() for f in args.filter)]
        if not workloads:
            print(f"ERROR: no workloads match filters: {args.filter}", file=sys.stderr)
            sys.exit(1)

    tmpdir = tempfile.mkdtemp(prefix="speedtest_")
    total = len(workloads)
    fixed_runs = args.runs  # None = adaptive, int = fixed
    measurement_time = args.measurement_time
    results = []
    profile_files = []
    is_tty = sys.stderr.isatty()

    if fixed_runs:
        mode_str = f"{fixed_runs} runs (fixed)"
    else:
        mode_str = f"adaptive (~{measurement_time:.0f}s per workload)"
    if is_tty:
        sys.stderr.write(f"Timing mode: {mode_str}\n")

    # ── Main loop ────────────────────────────────────────────────────────
    for idx, workload in enumerate(workloads, 1):
        name = workload["name"]
        slug = re.sub(r'[^a-z0-9]+', '_', name.lower()).strip('_')

        # Write temp files
        baml_file = os.path.join(tmpdir, f"{slug}.baml")
        py_file = os.path.join(tmpdir, f"{slug}.py")
        js_file = os.path.join(tmpdir, f"{slug}.js")
        for path, src in [(baml_file, workload["baml"]), (py_file, workload["python"]), (js_file, workload["js"])]:
            with open(path, "w") as f:
                f.write(src)

        row = {
            "name": name,
            "category": workload["category"],
            "source": {"baml": workload["baml"], "python": workload["python"], "js": workload["js"]},
        }

        if is_tty:
            sys.stderr.write(f"\n[{idx}/{total}] {name}\n")
            sys.stderr.flush()

        # Pack
        packed_baml = os.path.join(tmpdir, f"{slug}.packed")
        if not pack_baml(baml_cli, baml_file, packed_baml):
            if is_tty:
                sys.stderr.write("       SKIP (baml pack failed)\n")
            continue

        # Verify output
        def get_output(cmd):
            r = subprocess.run(cmd, capture_output=True, text=True)
            return r.stdout.strip().split("\n")[-1].strip() if r.returncode == 0 else None

        expected = get_output([packed_baml])
        if expected is None:
            if is_tty:
                sys.stderr.write("       SKIP (packed baml failed to run)\n")
            continue

        # Cross-language output verification
        checks = []
        if has_python:
            checks.append(("python3", ["python3", "-S", py_file]))
        if has_node:
            checks.append(("node", ["node", js_file]))
        if has_bun:
            checks.append(("bun", ["bun", js_file]))

        for lang, cmd in checks:
            out = get_output(cmd)
            if out is not None and out != expected:
                if is_tty:
                    sys.stderr.write(f"       MISMATCH: baml={expected}, {lang}={out}\n")
                row["mismatch"] = True

        # Time each runner
        def run_one(label, cmd):
            if is_tty:
                sys.stderr.write(f"       {label:>10s}: ")
                sys.stderr.flush()
            if fixed_runs:
                t = time_command_fixed(cmd, runs=fixed_runs)
            else:
                t = time_command_adaptive(cmd, measurement_time=measurement_time)
            if t:
                med = median(t)
                sd = stdev(t) if len(t) > 1 else 0.0
                n = len(t)
                if is_tty:
                    sys.stderr.write(
                        f"{fmt_ms_sd(med, sd):>14s}  "
                        f"({n} samples, min={fmt_ms(min(t))}, max={fmt_ms(max(t))})\n"
                    )
                return {"med": med, "sd": sd, "times": t}
            else:
                if is_tty:
                    sys.stderr.write("FAIL\n")
                return None

        row["baml"] = run_one("baml", [packed_baml])
        if has_python:
            row["python"] = run_one("python3", ["python3", "-S", py_file])
        if has_node:
            row["node"] = run_one("node", ["node", js_file])
        if has_bun:
            row["bun"] = run_one("bun", ["bun", js_file])

        # Profiling
        if args.profile and profile_baml_bin:
            def profile_one(label, prof_bin, prof_slug):
                if is_tty:
                    sys.stderr.write(f"       {label:>10s}: ")
                    sys.stderr.flush()
                json_path = os.path.join(tmpdir, f"{prof_slug}.json")
                r = subprocess.run(
                    ["samply", "record", "--save-only", "-o", json_path,
                     prof_bin, "run", "--file", baml_file, "-f", "main", "--", "main"],
                    capture_output=True, text=True,
                )
                if r.returncode != 0 or not os.path.isfile(json_path):
                    err = r.stderr.strip().splitlines()[-1] if r.stderr.strip() else f"exit {r.returncode}"
                    if is_tty:
                        sys.stderr.write(f"FAIL ({err})\n")
                    return None
                if is_tty:
                    sys.stderr.write(f"OK\n")
                return json_path

            p = profile_one("profile", profile_baml_bin, slug)
            if p:
                profile_files.append(p)

        results.append(row)

    # ── Print results ────────────────────────────────────────────────────
    _print_results(results, has_python, has_node, has_bun)

    # ── Save results ─────────────────────────────────────────────────────
    runners_used = ["baml"]
    if has_python:
        runners_used.append("python3")
    if has_node:
        runners_used.append("node")
    if has_bun:
        runners_used.append("bun")

    rdir = results_dir(args)
    data = build_run_data(args, results, runners_used, baml_cli=baml_cli)

    run_path = save_run(rdir, data, profile_files)

    # Always update baselines/<branch>/latest (and rotate last)
    tag = getattr(args, 'tag', None)
    bl_dir = save_baseline(rdir, run_path, data, tag=tag)

    branch = bl_dir.name

    print(f"\nResults saved to {branch}/latest", file=sys.stderr)
    if tag:
        print(f"Tagged as {branch}/{tag}", file=sys.stderr)
    print(f"\nTo compare:  speedtest compare {branch} <other-branch>", file=sys.stderr)
    print(f"To open UI:  speedtest open", file=sys.stderr)

    # Cleanup
    import shutil
    shutil.rmtree(tmpdir, ignore_errors=True)


# ── Output formatting ────────────────────────────────────────────────────

NAME_W = 30
NUM_W = 16


def _print_results(results, has_python, has_node, has_bun):
    cols = ["Benchmark", "baml"]
    if has_python:
        cols += ["python3", "baml/py"]
    if has_node:
        cols += ["node", "baml/node"]
    if has_bun:
        cols += ["bun", "baml/bun"]

    def med_of(e):
        return e["med"] if e else None

    def cell_timing(e):
        if e is None:
            return f"{'FAIL':>{NUM_W}s}"
        return f"{fmt_ms_sd(e['med'], e['sd']):>{NUM_W}s}"

    def cell_ratio(a, b):
        if a is None or b is None or b == 0:
            return f"{'—':>{NUM_W}s}"
        return f"{fmt_ratio(a, b):>{NUM_W}s}"

    def print_header():
        hdr = [f"{cols[0]:>{NAME_W}s}"] + [f"{c:>{NUM_W}s}" for c in cols[1:]]
        print("| " + " | ".join(hdr) + " |")
        sep = ["-" * NAME_W] + ["-" * NUM_W for _ in cols[1:]]
        print("| " + " | ".join(sep) + " |")

    def print_row(row):
        name = row["name"]
        display = name.split("::", 1)[-1] if "::" in name else name
        flag = " (!)" if row.get("mismatch") else ""
        cells = [f"{(display + flag):>{NAME_W}s}"]

        baml = row.get("baml")
        cells.append(cell_timing(baml))

        if has_python:
            py = row.get("python")
            cells.append(cell_timing(py))
            cells.append(cell_ratio(med_of(baml), med_of(py)))
        if has_node:
            nd = row.get("node")
            cells.append(cell_timing(nd))
            cells.append(cell_ratio(med_of(baml), med_of(nd)))
        if has_bun:
            bn = row.get("bun")
            cells.append(cell_timing(bn))
            cells.append(cell_ratio(med_of(baml), med_of(bn)))

        print("| " + " | ".join(cells) + " |")

    # Group by category
    categories = []
    seen = set()
    for row in results:
        cat = row["category"]
        if cat not in seen:
            categories.append(cat)
            seen.add(cat)

    print()
    for cat in categories:
        cat_rows = [r for r in results if r["category"] == cat]
        if not cat_rows:
            continue
        print(f"### {cat}\n")
        print_header()
        for row in cat_rows:
            print_row(row)
        print()
