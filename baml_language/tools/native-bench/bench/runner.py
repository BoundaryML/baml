"""The measuring side: build, verify, run, summarize. Runs where the trials run."""
import csv
import hashlib
import json
import os
import random
import shutil
import statistics
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path

from . import observer, oracle
from .rows import ROWS

FIELDS = ["schema", "protocol", "case", "scenario", "row", "level", "sink", "trial", "status", "coverage",
          "jobs", "warmup", "execution_wall_ns", "execution_cpu_ns", "process_wall_ns", "process_cpu_ns",
          "launch_to_ready_ns", "peak_rss_bytes", "drain_ns", "metadata_ns", "gc_execution", "gc_warmup",
          "gc_drain", "gc_counter", "telemetry_ok", "telemetry_bytes", "artifact_sha256", "input_sha256",
          "trial_path", "error", "observed_result"]
METRICS = ["execution_wall_ns", "execution_cpu_ns", "process_wall_ns", "process_cpu_ns",
           "launch_to_ready_ns", "peak_rss_bytes"]


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def log(*a):
    print(time.strftime("%H:%M:%S"), *a, file=sys.stderr, flush=True)


@dataclass
class Layout:
    """Where things are on the measuring machine."""
    root: Path            # bench root: tools, ref, artifacts, host
    stage: Path           # the shipped stage: cases/, native/, src.tgz
    out: Path             # results
    tools: dict           # row kind -> interpreter path (python, node, bun)

    @property
    def artifacts(self):
        return self.root / "artifacts"

    @property
    def references(self):
        return self.root / "ref"

    @property
    def host(self):
        return self.root / "artifacts" / "host"

    @property
    def cases(self):
        return self.stage / "cases"

    @property
    def source(self):
        return self.root / "src"

    def tool(self, name):
        return Path(self.tools[name])


def child_env(workers, level):
    env = {k: os.environ[k] for k in ("PATH", "HOME", "USER", "LANG") if k in os.environ}
    env.update({"LC_ALL": "C", "PYTHONHASHSEED": "0", "PYTHONDONTWRITEBYTECODE": "1",
                "BENCH_WORKERS": str(workers), "TOKIO_WORKER_THREADS": str(workers), "GOMAXPROCS": str(workers),
                "BAML_TELEMETRY": level, "BAML_NO_BYTECODE_CACHE": "1", "BAML_NO_DIAGNOSTICS_CACHE": "1",
                "BAML_AGENT_SKILL_CHECK": "off"})
    return env


# ── build ────────────────────────────────────────────────────────────────────

def unpack_source(layout):
    tgz = layout.stage / "src.tgz"
    if not tgz.exists():
        return None
    shutil.rmtree(layout.source, ignore_errors=True)
    layout.source.mkdir(parents=True)
    subprocess.run(["tar", "xzf", str(tgz), "-C", str(layout.source)], check=True)
    sha = (layout.source / "SHA").read_text().strip()
    log("source", sha)
    return sha


def freeze_control(layout, cases, machine):
    """The snapshot's native executables become the control row, once."""
    spec = machine.data.get("control") or {}
    if not spec.get("path"):
        return
    for case in cases:
        src = layout.root / spec["path"].format(case=case)
        dst = layout.root / spec["frozen_as"].format(case=case)
        if src.exists() and not dst.exists():
            shutil.copy(src, dst)


def compile_bytecode(layout, cases):
    for case in cases:
        out = layout.artifacts / case
        out.mkdir(parents=True, exist_ok=True)
        start = time.perf_counter()
        subprocess.run([str(layout.host), "compile", str(layout.cases / case / "workload.baml"), str(out / "program")],
                       check=True, env=child_env(1, "off"))
        (out / "program.compile_seconds").write_text(f"{time.perf_counter() - start:.3f}")
        log(case, "bytecode", (out / "program").stat().st_size, "bytes")


def build_native(layout, cases, jobs):
    """Clean release builds from the emitted projects; build time is a measurement."""
    builds = {}
    for case in cases:
        emit = layout.stage / "native" / case / "emit"
        out = layout.artifacts / case
        out.mkdir(parents=True, exist_ok=True)
        for stale in ("native", "native.stripped"):
            (out / stale).unlink(missing_ok=True)
        if not emit.exists():
            builds[case] = {"status": "not_emitted", "reason": (layout.stage / "native" / case / "emit.log").read_text()[-400:]
                            if (layout.stage / "native" / case / "emit.log").exists() else "no emitted project"}
            log(case, "native: not emitted")
            continue
        target = emit / "target"
        shutil.rmtree(target, ignore_errors=True)
        env = {**os.environ, "CARGO_TERM_COLOR": "never"}
        if case == cases[0]:
            subprocess.run(["cargo", "fetch", "--manifest-path", str(emit / "Cargo.toml")], env=env, capture_output=True)
        start = time.perf_counter()
        r = subprocess.run(["cargo", "build", "--release", "--offline", "--manifest-path", str(emit / "Cargo.toml"),
                            "--target-dir", str(target), "-j", str(jobs)], env=env, capture_output=True, text=True)
        seconds = time.perf_counter() - start
        (layout.out / f"build-{case}.log").write_text(r.stdout + r.stderr)
        if r.returncode != 0:
            builds[case] = {"status": "build_failed", "seconds": seconds}
            log(case, "native: BUILD FAILED")
            continue
        shutil.copy(target / "release" / "baml_native", out / "native")
        shutil.copy(out / "native", out / "native.stripped")
        subprocess.run(["strip", str(out / "native.stripped")], check=True)
        toolchain = subprocess.run(["rustc", "-V"], cwd=str(emit), capture_output=True, text=True, env=env).stdout.strip()
        builds[case] = {"status": "ok", "seconds": round(seconds, 3), "bytes": (out / "native").stat().st_size,
                        "stripped_bytes": (out / "native.stripped").stat().st_size, "sha256": digest(out / "native"), "jobs": jobs,
                        "rustc": toolchain, "lto": "fat", "codegen_units": 1, "panic": "abort"}
        shutil.copy(emit / "src" / "lib.rs", layout.out / f"generated-{case}.rs")
        log(case, f"native: {seconds:.1f}s, {builds[case]['stripped_bytes']} bytes stripped")
        shutil.rmtree(target, ignore_errors=True)
    return builds


# ── trials ───────────────────────────────────────────────────────────────────

def valid_protocol(observed, value, jobs, row):
    expected = ["ready", "result", "drained"] if row.baml else ["ready", "result"]
    if observed["event_order"] != expected or observed["malformed_lines"]:
        return False
    if type(value.get("jobs")) is not int or value["jobs"] != jobs:
        return False
    for counter in ("gc_execution", "gc_warmup"):
        count = value.get(counter)
        if count is not None and (type(count) is not int or count < 0):
            return False
        if row.baml and count is None and value.get("gc_counter") != "unavailable":
            return False
    for phase in ("wall", "cpu"):
        d, total = value.get(f"execution_{phase}_ns"), observed.get(f"process_{phase}_ns")
        if type(d) is not int or d < 0 or total is None or d > total + 1_000_000:
            return False
    return isinstance(value.get("result"), str)


def trial(layout, protocol, row, scenario, raw, jobs, warmup, directory, expected):
    fixture_path = directory.parent / (directory.name + ".input.json")
    fixture_path.parent.mkdir(parents=True, exist_ok=True)
    fixture_path.write_text(raw)
    recordings = directory / "recordings"
    cmd = row.command(layout, scenario.case, fixture_path, jobs, warmup, recordings)
    observed = observer.run(cmd, directory, child_env(protocol.workers, row.level), [protocol.cpu], protocol.timeout_s)
    value = observed.pop("result") or {}
    drain = observed.pop("drain") or {}
    if observed["status"] == "ok" and not valid_protocol(observed, value, jobs, row):
        observed["status"] = "invalid_protocol"
    r = dict(schema=1, protocol=protocol.name, case=scenario.case, scenario=scenario.name, row=row.name,
             level=row.level, sink=row.sink, jobs=jobs, warmup=warmup,
             artifact_sha256=digest(row.artifact(layout, scenario.case)), input_sha256=digest(fixture_path),
             trial_path=str(directory.relative_to(layout.out)), **observed,
             **{k: v for k, v in value.items() if k not in ("event", "result", "received_ns", "jobs")},
             **{k: v for k, v in drain.items() if k not in ("event", "received_ns")})
    if r["status"] == "ok":
        try:
            ok = oracle.matches(json.loads(value["result"]), expected) and value["jobs"] == jobs
        except (ValueError, KeyError, TypeError):
            ok = False
        if not ok:
            r["status"] = "wrong_result"
            r["observed_result"] = (value.get("result") or "")[:400]
    gc = r.get("gc_execution")
    r["coverage"] = ("gc_observed" if gc is not None and gc > 0 else "gc_not_observed" if gc == 0
                     else "gc_not_applicable" if not row.collector else "gc_unknown")
    if row.baml:
        if not drain.get("telemetry_ok", False) or drain.get("delivery_loss", 0):
            r["status"] = "telemetry_failure"
        has = recordings.exists() and any(recordings.iterdir())
        if (row.level == "off") == has:
            r["status"] = "telemetry_failure"
        if recordings.exists():
            r["telemetry_bytes"] = sum(f.stat().st_size for f in recordings.rglob("*") if f.is_file())
            shutil.rmtree(recordings, ignore_errors=True)
    return r


def write_csv(path, rows, fields):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fields, extrasaction="ignore")
        w.writeheader()
        w.writerows(rows)


def present_rows(layout, protocol, case, note):
    rows = []
    for name in protocol.rows:
        row = ROWS[name]
        if not row.supported:
            note.setdefault(case, []).append(name + " (unsupported)")
            continue
        if row.artifact(layout, case).exists() and (row.kind != "vm" or layout.host.exists()):
            rows.append(row)
        else:
            note.setdefault(case, []).append(name)
    return rows


def verify(layout, protocol, out):
    """Every row on every case's check set, one trial each, against the oracle."""
    failures, skipped = [], {}
    for case in protocol.cases:
        rows = present_rows(layout, protocol, case, skipped)
        n = 0
        for label, raw in oracle.checks(case):
            expected = oracle.oracle(case, raw, 1, 0)
            for row in rows:
                from .protocol import Scenario
                s = Scenario(name=f"check:{label}", case=case, jobs=1, warmup=0)
                r = trial(layout, protocol, row, s, raw, 1, 0, out / "verify" / case / f"{n:04d}-{row.name}", expected)
                n += 1
                if r["status"] != "ok":
                    failures.append(r)
        log(f"verify {case}: {n} trials, {len([f for f in failures if f['case'] == case])} failed")
    write_csv(out / "verify.csv", failures, FIELDS)
    return failures, skipped


def run(layout, protocol, out, rows_filter=None):
    all_rows = []
    skipped = {}
    sched = []
    for scenario in protocol.scenarios:
        raw = oracle.fixture(*scenario.fixture_args(protocol.seed))
        expected = oracle.oracle(scenario.case, raw, scenario.jobs, scenario.warmup)
        for row in present_rows(layout, protocol, scenario.case, skipped):
            if rows_filter and row.name not in rows_filter:
                continue
            for t in range(protocol.trials):
                sched.append((scenario, row, raw, expected, t))
    random.Random(protocol.seed).shuffle(sched)
    log(f"{len(sched)} trials scheduled; load {Path('/proc/loadavg').read_text().split()[:3]}")
    counters = {}
    for n, (scenario, row, raw, expected, t) in enumerate(sched):
        d = out / "runtime" / scenario.name / f"{n:05d}-{row.name}"
        try:
            r = trial(layout, protocol, row, scenario, raw, scenario.jobs, scenario.warmup, d, expected)
        except Exception as error:  # noqa: BLE001
            r = dict(schema=1, protocol=protocol.name, case=scenario.case, scenario=scenario.name, row=row.name,
                     level=row.level, sink=row.sink, jobs=scenario.jobs, warmup=scenario.warmup,
                     status="harness_error", error=str(error)[:400], trial_path=str(d.relative_to(out)))
        r["trial"] = t
        all_rows.append(r)
        (d / "sample.json").parent.mkdir(parents=True, exist_ok=True)
        (d / "sample.json").write_text(json.dumps(r, indent=1) + "\n")
        k = (scenario.name, row.name)
        counters[k] = counters.get(k, 0) + 1
        if (n + 1) % 100 == 0 or n + 1 == len(sched):
            log(f"{n + 1}/{len(sched)}")
            write_csv(out / "trials.csv", all_rows, FIELDS)
    write_csv(out / "trials.csv", all_rows, FIELDS)
    return all_rows, skipped


def summarize(rows, protocol):
    """summary.json: per scenario x row, each metric's n, median, min, max; per job for execution."""
    table = {}
    for r in rows:
        if r["status"] != "ok":
            table.setdefault(r["scenario"], {}).setdefault(r["row"], {}).setdefault("failed", 0)
            table[r["scenario"]][r["row"]]["failed"] += 1
            continue
        slot = table.setdefault(r["scenario"], {}).setdefault(r["row"], {"jobs": int(r["jobs"]), "n": 0, "failed": 0})
        slot["n"] += 1
        for m in METRICS:
            if r.get(m) not in (None, ""):
                slot.setdefault(m, []).append(float(r[m]))
        if r.get("telemetry_bytes") not in (None, ""):
            slot.setdefault("telemetry_bytes", []).append(float(r["telemetry_bytes"]))
    summary = {}
    for scen, by_row in table.items():
        summary[scen] = {}
        for row, slot in by_row.items():
            entry = {"n": slot.get("n", 0), "failed": slot.get("failed", 0), "jobs": slot.get("jobs")}
            for m in METRICS + ["telemetry_bytes"]:
                v = slot.get(m)
                if v:
                    med = statistics.median(v)
                    entry[m] = {"median": med, "min": min(v), "max": max(v)}
                    if m.startswith("execution") and slot.get("jobs"):
                        entry[m]["per_job"] = {k: entry[m][k] / slot["jobs"] for k in ("median", "min", "max")}
            summary[scen][row] = entry
    return summary
