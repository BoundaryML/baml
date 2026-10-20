"""The run loop: build, verify, measure, summarize, write the bundle.

    1. Build: pack every workload's main.baml with the speedtest driver into one
       executable per BAML backend in the run (pack time and executable size
       are results).
    2. Verify: run every (workload, row) once and compare its result with the
       workload's `expect`; a row that fails is reported, never measured.
    3. Measure: `trials` rounds; each round runs every (workload, row) once, in
       a seeded shuffled order, each in a fresh process. Every trial's result
       is checked again.
    4. Summarize and write the bundle: meta.json (baselines, compare, UI),
       trials.csv (every trial), kpis.json and kpis.html (the KPI report).
"""

import csv
import os
import queue
import random
import re
import shutil
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from statistics import median, stdev

from speedtest import kpis, measure, rows as rowmod
from speedtest.fmt import fmt_ratio
from speedtest.loader import load_workloads, select
from speedtest.storage import build_run_data, results_dir, save_baseline, save_run

DRIVER = rowmod.DRIVERS / "bench_driver.baml"
SETUP_DRIVER = rowmod.DRIVERS / "bench_driver_setup.baml"


def _has_setup(baml):
    return bool(re.search(r"^function setup\(", baml, re.M) and re.search(r"^function run\(", baml, re.M))


def _repo_root():
    d = Path(__file__).resolve()
    while d != d.parent:
        if (d / "Cargo.toml").exists() and (d / "crates").is_dir():
            return d
        d = d.parent
    sys.exit("cannot find baml_language/ (the Cargo workspace) above the speedtest package")


def _log(msg):
    print(msg, file=sys.stderr, flush=True)


def _cli_env():
    # The CLI refuses to run inside coding agents without an installed skill;
    # the harness is not an agent session.
    return {**os.environ, "DEV_BAML_CLI_DISABLE_AGENT_DETECTION": "1"}


def build_baml(cli, workload, stage, backend, repo, build_dir):
    """Pack main.baml plus the driver into one executable with `backend`. Returns a build record."""
    project = stage / backend / workload["id"].replace("/", "__")
    src = project / "baml_src"
    src.mkdir(parents=True)
    # Bind BAML_TELEMETRY so the packed binary honours the row's level; an
    # unbound pack runs at the baked default (medium) whatever the env says.
    (project / "baml.toml").write_text(
        '[package]\nname = "speedtest_workload"\n[pack.env_var_names]\nBAML_TELEMETRY = "BAML_TELEMETRY"\n'
    )
    shutil.copy(workload["files"]["baml"], src / "main.baml")
    shutil.copy(SETUP_DRIVER if _has_setup(workload["baml"]) else DRIVER, src / "speedtest_driver.baml")
    out = project / "workload"
    cmd = [str(cli), "--no-progress", "--color", "never", "pack"]
    if backend == "rust":
        # The runtime compiles once per build directory; each workload's crate reuses it.
        cmd += ["--rust", "--rust-workspace", str(repo), "--rust-build-dir", str(build_dir)]
    start = time.perf_counter()
    r = subprocess.run(cmd + ["speedtest_driver", "-o", str(out)],
                       cwd=project, capture_output=True, text=True, env=_cli_env(), stdin=subprocess.DEVNULL)
    seconds = time.perf_counter() - start
    if r.returncode != 0 or not out.exists():
        lines = [l for l in r.stderr.strip().splitlines() if l.strip() and not l.startswith("warning:")]
        return {"status": "failed", "error": (lines[-1] if lines else f"exit {r.returncode}")[:400]}
    return {"status": "ok", "artifact": str(out), "seconds": seconds, "bytes": out.stat().st_size}


def _build_all(cli, workloads, backends, stage, repo, jobs):
    """Pack every (workload, backend), `jobs` at a time. Rust packs are mostly a
    single-threaded LTO link, so parallel slots each get their own build
    directory and pay for the runtime once."""
    dirs = queue.Queue()
    for slot in range(max(jobs, 1)):
        dirs.put(stage / f"rust-build-{slot}")

    def one(w, backend):
        build_dir = dirs.get()
        try:
            return (w["id"], backend), build_baml(cli, w, stage, backend, repo, build_dir)
        finally:
            dirs.put(build_dir)

    with ThreadPoolExecutor(max_workers=max(jobs, 1)) as pool:
        return dict(pool.map(lambda wb: one(*wb), [(w, b) for w in workloads for b in backends]))


def _stats(values):
    values = [v for v in values if v is not None]
    if not values:
        return None
    return {"median": median(values), "min": min(values), "max": max(values),
            "stdev": stdev(values) if len(values) > 1 else 0.0, "n": len(values)}


def _dir_bytes(path):
    return sum(f.stat().st_size for f in Path(path).rglob("*") if f.is_file()) if Path(path).exists() else 0


class Cell:
    """One (workload, row): its verification outcome and trials."""

    def __init__(self, workload, row):
        self.workload, self.row = workload, row
        self.status, self.reason, self.trials = "pending", None, []

    def fail(self, reason):
        self.status, self.reason = "failed", reason

    def summary(self):
        ok = [t for t in self.trials if t.ok]
        out = {"status": self.status, "reason": self.reason, "trials": len(self.trials), "failed_trials": len(self.trials) - len(ok)}
        if ok:
            out.update(per_job_ns=_stats([t.per_job_ns for t in ok]), process_ns=_stats([t.process_ns for t in ok]),
                       cpu_ns=_stats([t.cpu_ns for t in ok]), peak_rss_bytes=_stats([t.peak_rss_bytes for t in ok]),
                       jobs=ok[0].jobs)
            if self.row.is_baml:
                out["recording_bytes"] = _stats([t.extra.get("recording_bytes") for t in ok])
        return out


def _trial(cell, artifact, jobs, warmup, scratch, cpu):
    home = Path(tempfile.mkdtemp(prefix="home-", dir=scratch))
    try:
        t = measure.run(cell.row.command(cell.workload, artifact, jobs, warmup), env=cell.row.env(os.environ, home), cpu=cpu)
        if cell.row.is_baml:
            t.extra["recording_bytes"] = _dir_bytes(home / "btel")
    finally:
        shutil.rmtree(home, ignore_errors=True)
    if t.ok and t.result != cell.workload["expect"]:
        t.ok, t.error = False, f"result {t.result[:80]!r} != expected {cell.workload['expect'][:80]!r}"
    return t


def _fmt_ns(ns):
    if ns is None:
        return "—"
    if ns >= 1e9:
        return f"{ns / 1e9:.2f}s"
    if ns >= 1e6:
        return f"{ns / 1e6:.1f}ms" if ns >= 1e7 else f"{ns / 1e6:.2f}ms"
    return f"{ns / 1e3:.1f}µs"


def print_table(workloads, row_list, cells):
    primary = next((r for r in row_list if r.key == rowmod.PRIMARY), row_list[0])
    others = [r for r in row_list if r is not primary]
    head = ["workload", primary.label] + [c for r in others for c in (r.label, f"vs {r.label}")]
    table = [head]
    for w in workloads:
        p = cells[(w["id"], primary.key)].summary()
        pm = p.get("per_job_ns", {}).get("median") if p.get("per_job_ns") else None
        line = [w["name"], _fmt_ns(pm) if pm else p["status"]]
        for r in others:
            s = cells[(w["id"], r.key)].summary()
            m = s["per_job_ns"]["median"] if s.get("per_job_ns") else None
            line += [_fmt_ns(m) if m else s["status"], fmt_ratio(pm, m) if pm and m else "—"]
        table.append(line)
    widths = [max(len(str(row[i])) for row in table) for i in range(len(head))]
    for i, row in enumerate(table):
        print("  ".join(str(c).ljust(widths[j]) if j == 0 else str(c).rjust(widths[j]) for j, c in enumerate(row)))
        if i == 0:
            print("  ".join("-" * w for w in widths))
    print(f"\nper-job time, median of trials; 'vs X' is {primary.label} time over X's (above 1x: BAML is slower).")


def _machine():
    info = {"platform": sys.platform, "cores": os.cpu_count(), "kernel": os.uname().release}
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.lower().startswith(("model name", "hardware")):
                info["cpu"] = line.split(":", 1)[1].strip()
                break
    except OSError:
        pass
    if "cpu" not in info:
        r = subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True, text=True)
        info["cpu"] = r.stdout.strip() or os.uname().machine
    return info


def cmd_run(args):
    repo = _repo_root()
    cli = Path(args.baml or repo / "target" / "release" / "baml-cli").resolve()
    if args.build:
        cmd = ["cargo", "build", "--release", "-p", "baml_pack_host", "-p", "baml_cli"]
        _log(f"$ {' '.join(cmd)}")
        if subprocess.run(cmd, cwd=repo).returncode != 0:
            sys.exit(1)
    if not cli.is_file():
        sys.exit(f"no baml-cli at {cli}; build it with `cargo build --release -p baml_cli -p baml_pack_host` or pass --build")

    workloads = load_workloads()
    missing = sorted(kpis.workload_names() - {w["name"] for w in workloads})
    if missing:
        sys.exit(f"kpis.toml names workloads that do not exist: {', '.join(missing)}")
    if args.kpi:
        wanted = kpis.workload_names()
        workloads = [w for w in workloads if w["name"] in wanted]
    workloads = select(workloads, args.filter)
    if not workloads:
        sys.exit(f"no workloads match {args.filter}")

    row_list = rowmod.resolve(args.rows.split(",") if args.rows else None, telemetry=args.telemetry or args.kpi)
    versions = {}
    for r in row_list:
        if not r.is_baml and r.kind not in versions:
            versions[r.kind] = rowmod.toolchain_version(r.kind)
    trials_n = args.trials or 5

    stage = Path(tempfile.mkdtemp(prefix="speedtest-"))
    cells = {(w["id"], r.key): Cell(w, r) for w in workloads for r in row_list}
    builds = {}
    started = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    load_start = os.getloadavg()
    if load_start[0] > (os.cpu_count() or 1) / 4:
        _log(f"warning: load average {load_start[0]:.1f} on {os.cpu_count()} cores; results will be noisy")
    try:
        backends = sorted({r.backend for r in row_list if r.is_baml})
        _log(f"building {len(workloads)} workloads as {', '.join(backends)} with {cli}")
        builds = _build_all(cli, workloads, backends, stage, repo, args.build_jobs)
        for (wid, backend), b in builds.items():
            if b["status"] != "ok":
                _log(f"  {wid} [{backend}]: pack failed: {b['error']}")
        builds = {w["id"]: {backend: builds[(w["id"], backend)] for backend in backends} for w in workloads}

        _log(f"verifying {len(cells)} (workload, row) pairs")
        for (wid, _), cell in cells.items():
            if not cell.row.is_baml and versions.get(cell.row.kind) is None:
                cell.status, cell.reason = "unavailable", f"{cell.row.kind} is not installed"
                continue
            build = builds[wid].get(cell.row.backend, {})
            if cell.row.is_baml and build["status"] != "ok":
                cell.fail(f"pack failed: {build['error']}")
                continue
            t = _trial(cell, build.get("artifact"), 1, 0, stage, args.cpu)
            if t.ok:
                cell.status = "ok"
            else:
                cell.fail(t.error)
                _log(f"  {cell.workload['name']} [{cell.row.key}]: {t.error}")

        live = [c for c in cells.values() if c.status == "ok"]
        rng = random.Random(args.seed)
        _log(f"measuring: {trials_n} trials x {len(live)} pairs, shuffled (seed {args.seed})")
        for i in range(trials_n):
            order = live[:]
            rng.shuffle(order)
            for cell in order:
                w = cell.workload
                jobs = args.jobs or w["jobs"]
                artifact = builds[w["id"]].get(cell.row.backend, {}).get("artifact")
                t = _trial(cell, artifact, jobs, w["warmup"], stage, args.cpu)
                cell.trials.append(t)
                if not t.ok:
                    _log(f"  trial {i + 1}: {w['name']} [{cell.row.key}]: {t.error}")
            _log(f"  round {i + 1}/{trials_n} done")

        if args.profile:
            _profile(args, cli, workloads, stage)
    finally:
        shutil.rmtree(stage, ignore_errors=True)

    print()
    print_table(workloads, row_list, cells)
    _save(args, cli, workloads, row_list, cells, builds, versions, trials_n,
          {"started": started, "finished": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
           "loadavg_start": load_start, "loadavg_end": os.getloadavg()})


def _profile(args, cli, workloads, stage):
    prof = Path(args.profile_baml or str(cli).replace("/release/", "/profiling/"))
    if not shutil.which("samply") or not prof.is_file():
        _log(f"--profile needs samply on PATH and a profiling baml-cli at {prof}; skipping")
        return
    out = stage / "profiles"
    out.mkdir(exist_ok=True)
    for w in workloads:
        path = out / f"{w['id'].replace('/', '__')}.json"
        subprocess.run(["samply", "record", "--save-only", "-o", str(path), str(prof), "run", "--file",
                        str(w["files"]["baml"]), "main"], capture_output=True, env=_cli_env())
    args._profile_files = sorted(str(p) for p in out.glob("*.json"))


def _save(args, cli, workloads, row_list, cells, builds, versions, trials_n, timing):
    summaries = {w["id"]: {r.key: cells[(w["id"], r.key)].summary() for r in row_list} for w in workloads}

    # meta.json keeps the shape baselines, `compare` and the UI read: per
    # runner, seconds per job.
    legacy = {"vm-off": "baml", "python": "python", "node": "node", "bun": "bun"}
    results = []
    for w in workloads:
        entry = {"name": w["name"], "category": w["category"],
                 "source": {"baml": w["baml"], "python": w["python"], "js": w["js"]}}
        for key, name in legacy.items():
            if (w["id"], key) not in cells:
                continue
            ok = [t.per_job_ns / 1e9 for t in cells[(w["id"], key)].trials if t.ok]
            entry[name] = {"med": median(ok), "sd": stdev(ok) if len(ok) > 1 else 0.0, "times": ok} if ok else None
        results.append(entry)
    args.runs, args.measurement_time = trials_n, 0
    data = build_run_data(args, results, [legacy[r.key] for r in row_list if r.key in legacy], baml_cli=str(cli))
    data.update(protocol="speedtest-2", trials=trials_n, seed=args.seed, cpu=args.cpu, machine=_machine(),
                toolchains=versions, rows=[r.key for r in row_list], builds=builds, summary=summaries, **timing)

    rdir = results_dir(args)
    run_path = save_run(rdir, data, getattr(args, "_profile_files", None))
    with open(run_path / "trials.csv", "w", newline="") as f:
        out = csv.writer(f)
        out.writerow(["workload", "row", "trial", "ok", "jobs", "per_job_ns", "elapsed_ns", "process_ns", "cpu_ns",
                      "peak_rss_bytes", "recording_bytes", "error"])
        for (wid, key), cell in cells.items():
            for i, t in enumerate(cell.trials):
                out.writerow([wid, key, i, int(t.ok), t.jobs, t.per_job_ns, t.elapsed_ns, t.process_ns, t.cpu_ns,
                              t.peak_rss_bytes, t.extra.get("recording_bytes"), t.error or ""])
    page = kpis.render(run_path, data, workloads, row_list)
    bl = save_baseline(rdir, run_path, data, tag=args.tag)
    if args.out:
        out_dir = Path(args.out)
        out_dir.mkdir(parents=True, exist_ok=True)
        for name in ("meta.json", "trials.csv", "kpis.json", "kpis.html"):
            shutil.copy(run_path / name, out_dir / name)
        page = out_dir / "kpis.html"

    failed = sum(1 for c in cells.values() if c.status == "failed") + sum(s["failed_trials"] for ws in summaries.values() for s in ws.values())
    _log(f"\nreport:  {page}")
    _log(f"run:     {run_path}  (baseline {bl.name}/latest{', tag ' + args.tag if args.tag else ''})")
    _log(f"compare: speedtest compare {bl.name} <other>")
    if failed:
        _log(f"{failed} failure(s); see the report")
        sys.exit(1)
