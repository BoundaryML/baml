#!/usr/bin/env python3
"""BTEL query prototype measurements. Standard library only; Linux.

record: fixed-work recording in fresh processes. Variants: telemetry off (base
        and branch binaries), recording on the base commit, recording on this
        branch, and this branch with a `baml query` reader once per second.
query:  first query with no index, unchanged-source queries in new processes
        and on one reused connection, CAS filters, index size.
growth: refresh cost for a fixed small addition to increasing history.

Cache conditions: "warm" runs follow earlier reads of the same files. "evicted"
runs first fsync and drop the source and index files from the page cache with
POSIX_FADV_DONTNEED; binaries and libraries stay cached.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import shutil
import subprocess
import threading
import time

WORKLOADS = ["tiny", "dense", "spawn", "capture-repeat", "capture-unique"]
QUERIES = {
    "count": "SELECT count(*) FROM calls",
    # Every column: SQLite evaluates each per-execution subquery before sorting.
    "executions": "SELECT * FROM executions ORDER BY started_at_ms DESC LIMIT 20",
    "stats": "SELECT fqn, sum(call_count) AS calls, sum(total_duration_ns) AS total_ns "
    "FROM function_stats GROUP BY fqn ORDER BY calls DESC",
    "slowest": "SELECT call_id, fqn, duration_ns FROM calls ORDER BY duration_ns DESC LIMIT 10",
    "cas_filter": "SELECT count(*) FROM calls WHERE args['n'] = 7",
    "cas_render": "SELECT args['n'], output FROM calls WHERE fqn LIKE '%capture' LIMIT 50",
}
METADATA_QUERIES = ["count", "executions", "stats", "slowest"]


class Child:
    def __init__(self, code, out, err, wall_s, usage):
        self.code, self.out, self.err, self.wall_s, self.usage = code, out, err, wall_s, usage

    def resources(self):
        u = self.usage
        return {
            "wall_ms": self.wall_s * 1e3,
            "cpu_s": u.ru_utime + u.ru_stime,
            "peak_rss_bytes": u.ru_maxrss * 1024,
            # Linux charges reads that reach storage and pages dirtied by writes.
            "storage_read_bytes": u.ru_inblock * 512,
            "written_bytes": u.ru_oublock * 512,
        }


def run_child(command, env=None):
    """Run to completion and keep the child's own rusage (wait4)."""
    start = time.perf_counter()
    process = subprocess.Popen([str(c) for c in command], env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    output = {}

    def drain(name, stream):
        output[name] = stream.read().decode(errors="replace")

    readers = [threading.Thread(target=drain, args=pair)
               for pair in (("out", process.stdout), ("err", process.stderr))]
    for reader in readers:
        reader.start()
    _, status, usage = os.wait4(process.pid, 0)
    wall_s = time.perf_counter() - start
    process.returncode = os.waitstatus_to_exitcode(status)
    for reader in readers:
        reader.join()
    return Child(process.returncode, output["out"], output["err"], wall_s, usage)


def files_under(root):
    return [p for p in Path(root).rglob("*") if p.is_file()] if Path(root).exists() else []


def evict(root):
    for path in files_under(root):
        fd = os.open(path, os.O_RDONLY)
        try:
            os.fsync(fd)
            os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
        finally:
            os.close(fd)


def resident_bytes(root):
    """Page-cache residency of `root`'s files, via util-linux fincore."""
    paths = [str(p) for p in files_under(root)]
    if not paths or not shutil.which("fincore"):
        return None
    total = 0
    for start in range(0, len(paths), 500):
        out = subprocess.run(["fincore", "--bytes", "--noheadings", "--raw", "-o", "RES",
                              *paths[start:start + 500]], capture_output=True, text=True).stdout
        total += sum(int(line) for line in out.split())
    return total


def source_stats(project):
    btel = Path(project) / ".baml/btel"
    recordings = files_under(btel / "recordings")
    blobs = files_under(btel / "cas")
    index = {p.name: p.stat().st_size for p in btel.glob("query.sqlite*")}
    return {
        "recordings": len({p.parent for p in recordings if p.suffix == ".btel"}),
        "recording_files": sum(p.suffix == ".btel" for p in recordings),
        "recording_bytes": sum(p.stat().st_size for p in recordings),
        "cas_blobs": len(blobs),
        "cas_bytes": sum(p.stat().st_size for p in blobs),
        "index_files": index,
    }


def delete_index(project):
    for path in (Path(project) / ".baml/btel").glob("query.sqlite*"):
        path.unlink()


class Bench:
    def __init__(self, args):
        self.args = args
        self.raw = (args.output / "raw.jsonl").open("a")

    def emit(self, record):
        record["utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        self.raw.write(json.dumps(record) + "\n")
        self.raw.flush()

    def record(self, binary, mode, workload, roots, project, extra=()):
        env = dict(os.environ, BAML_TELEMETRY="off" if mode == "off" else "medium")
        child = run_child([binary, mode, workload, roots, project, *extra], env=env)
        if child.code:
            raise RuntimeError(f"{binary} {mode} {workload}:\n{child.out}\n{child.err}")
        return json.loads(child.out.strip().splitlines()[-1])

    def query(self, project, sql, *flags):
        # As the CLI's own end-to-end tests do: no agent-skill gate for the harness.
        env = dict(os.environ, BAML_AGENT_SKILL_CHECK="off")
        child = run_child([self.args.baml, "query", "--from", project, "--format", "json",
                           *flags, sql], env=env)
        # 0 complete, 1 incomplete evidence (live or unavailable values).
        if child.code not in (0, 1):
            raise RuntimeError(f"query {sql!r} exited {child.code}:\n{child.err}")
        result = json.loads(child.out)
        return {
            "exit_code": child.code,
            "rows": len(result["rows"]),
            "first_row": result["rows"][0] if result["rows"] else None,
            "status": result["outcome"]["status"],
            "refresh": result["outcome"].get("refresh"),
            "metrics": result["outcome"]["query"],
            "diagnostics": result["outcome"]["diagnostics"],
            **child.resources(),
        }

    # -- record ---------------------------------------------------------------

    def calibrate(self, workload):
        project = self.args.storage_root / f"pilot-{workload}"
        shutil.rmtree(project, ignore_errors=True)
        pilot_roots = 512
        pilot = self.record(self.args.bench, "btel", workload, pilot_roots, project)
        shutil.rmtree(project)
        seconds = pilot["execution_s"] + pilot["drain_ms"] / 1e3
        return max(512, round(pilot_roots * self.args.seconds / seconds / 8) * 8)

    def reader(self, project, stop, samples):
        started = time.monotonic()
        tick = 1
        while not stop.wait(max(0.0, started + tick - time.monotonic())):
            samples.append(self.query(project, QUERIES["count"]))
            tick = int(time.monotonic() - started) + 1

    def run_record(self, roots):
        rng = random.Random(self.args.seed)
        variants = {
            "off-base": (self.args.base_bench, "off"),
            "off": (self.args.bench, "off"),
            "record-base": (self.args.base_bench, "btel"),
            "record": (self.args.bench, "btel"),
            "record+reader": (self.args.bench, "btel"),
        }
        for workload in self.args.workloads:
            print(f"record {workload}: {roots[workload]} roots", flush=True)
            for repeat in range(self.args.repeats):
                order = list(variants)
                rng.shuffle(order)
                for variant in order:
                    binary, mode = variants[variant]
                    project = self.args.storage_root / f"record-{variant}"
                    shutil.rmtree(project, ignore_errors=True)
                    samples, stop = [], threading.Event()
                    reader = None
                    if variant == "record+reader":
                        reader = threading.Thread(target=self.reader, args=(project, stop, samples))
                        reader.start()
                    row = self.record(binary, mode, workload, roots[workload], project)
                    if reader:
                        stop.set()
                        reader.join()
                        # The final refresh applies whatever the reader had not seen.
                        samples.append(self.query(project, QUERIES["count"]))
                    self.emit({"experiment": "record", "variant": variant, "repeat": repeat,
                               **row, "reader": samples or None})
                    shutil.rmtree(project, ignore_errors=True)

    # -- query ----------------------------------------------------------------

    def build_fixture(self, name, workload, roots, extra=()):
        project = self.args.storage_root / "fixtures" / name
        if not (project / ".baml/btel/recordings").exists():
            shutil.rmtree(project, ignore_errors=True)
            row = self.record(self.args.bench, "btel", workload, roots, project, extra)
            self.emit({"experiment": "fixture", "fixture": name, **row})
        return project

    def same_process(self, project, fixture, cache):
        child = run_child([self.args.query_bench, project, self.args.iterations,
                           *QUERIES.values()])
        if child.code:
            raise RuntimeError(f"query_bench {fixture}:\n{child.err}")
        self.emit({"experiment": "query", "phase": f"same-process-{cache}", "fixture": fixture,
                   **json.loads(child.out), **child.resources()})

    def run_query(self, fixtures):
        rng = random.Random(self.args.seed + 1)
        for fixture, project in fixtures.items():
            print(f"query {fixture}: {source_stats(project)}", flush=True)
            before = resident_bytes(project / ".baml")
            evict(project / ".baml")
            self.emit({"experiment": "query", "phase": "eviction-check", "fixture": fixture,
                       "resident_before": before, "resident_after": resident_bytes(project / ".baml")})
            names = list(QUERIES)
            for cache in ("warm", "evicted"):
                # Forced rebuild: every query starts with no index.
                for repeat in range(self.args.query_repeats):
                    rng.shuffle(names)
                    for name in names:
                        delete_index(project)
                        if cache == "evicted":
                            evict(project / ".baml")
                        result = self.query(project, QUERIES[name])
                        self.emit({"experiment": "query", "phase": f"first-{cache}",
                                   "fixture": fixture, "query": name, "repeat": repeat,
                                   **result, "source": source_stats(project)})
                # Unchanged source, new process per query.
                for repeat in range(self.args.query_repeats):
                    rng.shuffle(names)
                    for name in names:
                        if cache == "evicted":
                            evict(project / ".baml")
                        result = self.query(project, QUERIES[name])
                        refresh = result["refresh"]
                        assert refresh["files_decoded"] == 0, refresh
                        assert refresh["files_unchanged"] == refresh["files_seen"], refresh
                        self.emit({"experiment": "query", "phase": f"unchanged-{cache}",
                                   "fixture": fixture, "query": name, "repeat": repeat,
                                   **result})
            if self.args.query_bench:
                delete_index(project)
                self.same_process(project, fixture, "build")
                self.same_process(project, fixture, "unchanged")
            self.emit({"experiment": "query", "phase": "size", "fixture": fixture,
                       **source_stats(project), **sqlite_pragmas(project)})

    # -- growth ---------------------------------------------------------------

    def add(self, project, roots):
        return self.record(self.args.bench, "btel", self.args.growth_workload, roots, project)

    def run_growth(self):
        project = self.args.storage_root / "growth"
        shutil.rmtree(project, ignore_errors=True)
        project.mkdir(parents=True)
        bulks = 0
        for level in self.args.growth_levels:
            while bulks < level:
                self.add(project, self.args.growth_bulk_roots)
                bulks += 1
            # Apply the bulk outside the measurement.
            self.query(project, QUERIES["count"])
            history = source_stats(project)
            print(f"growth {level} bulks: {history}", flush=True)
            for repeat in range(self.args.query_repeats):
                added = self.add(project, self.args.growth_small_roots)
                result = self.query(project, QUERIES["count"])
                self.emit({"experiment": "growth", "phase": "incremental", "bulks": level,
                           "repeat": repeat, "added": added, "history": history, **result})
            delete_index(project)
            result = self.query(project, QUERIES["count"])
            self.emit({"experiment": "growth", "phase": "rebuild", "bulks": level,
                       "history": source_stats(project), **result})
            self.emit({"experiment": "growth", "phase": "size", "bulks": level,
                       **source_stats(project), **sqlite_pragmas(project)})
        shutil.rmtree(project)


def sqlite_pragmas(project):
    import sqlite3
    path = Path(project) / ".baml/btel/query.sqlite"
    if not path.exists():
        return {}
    # Immutable: read the main file without touching the WAL or lock files.
    conn = sqlite3.connect(f"file:{path}?immutable=1", uri=True)
    try:
        return {"sqlite_" + name: conn.execute(f"PRAGMA {name}").fetchone()[0]
                for name in ("page_size", "page_count", "freelist_count", "journal_mode")}
    finally:
        conn.close()


def environment(args):
    def digest(path):
        return hashlib.sha256(Path(path).read_bytes()).hexdigest() if path else None

    def output(*command):
        try:
            return subprocess.check_output(command, text=True, stderr=subprocess.DEVNULL).strip()
        except (OSError, subprocess.CalledProcessError):
            return None

    cpu = None
    if Path("/proc/cpuinfo").exists():
        cpu = next((line.split(":", 1)[1].strip() for line in
                    Path("/proc/cpuinfo").read_text().splitlines()
                    if line.startswith("model name")), None)
    return {
        "binaries": {name: {"path": str(getattr(args, name)), "sha256": digest(getattr(args, name))}
                     for name in ("baml", "bench", "base_bench", "query_bench")
                     if getattr(args, name)},
        "platform": platform.platform(), "cpu": cpu, "cpu_count": os.cpu_count(),
        "load_average": os.getloadavg(),
        "storage_root": str(args.storage_root.resolve()),
        "filesystem": output("findmnt", "-T", str(args.storage_root), "-no", "SOURCE,FSTYPE,OPTIONS"),
        "git_head": output("git", "rev-parse", "HEAD"),
        "git_diff_stat": output("git", "diff", "--stat"),
        "base": "778a5f002e084e4c534af7b4a58ee066b564eed1",
        "args": {k: str(v) for k, v in vars(args).items()},
        "utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--baml", type=Path, required=True, help="release baml-cli")
    parser.add_argument("--bench", type=Path, required=True, help="this branch's btel_record_bench")
    parser.add_argument("--base-bench", type=Path, help="btel_record_bench built on the base commit")
    parser.add_argument("--query-bench", type=Path, help="baml_query_btel query_bench example")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--storage-root", type=Path, default=Path("target/btel-query-bench"),
                        help="recording storage; a real disk, not tmpfs")
    parser.add_argument("--experiments", nargs="+", default=["record", "query", "growth"],
                        choices=["record", "query", "growth"])
    parser.add_argument("--workloads", nargs="+", choices=WORKLOADS, default=WORKLOADS)
    parser.add_argument("--seconds", type=float, default=5, help="target recording seconds per run")
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--query-repeats", type=int, default=5)
    parser.add_argument("--iterations", type=int, default=20, help="same-process repeats")
    parser.add_argument("--growth-workload", choices=WORKLOADS, default="capture-repeat")
    parser.add_argument("--growth-bulk-roots", type=int, default=16384)
    parser.add_argument("--growth-small-roots", type=int, default=256)
    parser.add_argument("--growth-levels", type=int, nargs="+", default=[0, 1, 3, 7, 15, 31])
    parser.add_argument("--segment-bytes", type=int, default=16 * 1024,
                        help="file size for the many-segment fixture")
    parser.add_argument("--seed", type=int, default=4953)
    args = parser.parse_args()
    if "record" in args.experiments and not args.base_bench:
        parser.error("record needs --base-bench")
    for name in ("baml", "bench", "base_bench", "query_bench"):
        if getattr(args, name):
            setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=True)
    args.storage_root.mkdir(parents=True, exist_ok=True)
    (args.output / "environment.json").write_text(json.dumps(environment(args), indent=2) + "\n")

    bench = Bench(args)
    roots = {}
    roots_file = args.output / "roots.json"
    if roots_file.exists():
        roots = json.loads(roots_file.read_text())
    for workload in args.workloads:
        if workload not in roots:
            roots[workload] = bench.calibrate(workload)
    roots_file.write_text(json.dumps(roots, indent=2) + "\n")

    if "record" in args.experiments:
        bench.run_record(roots)
    if "query" in args.experiments:
        fixtures = {w: bench.build_fixture(w, w, roots[w]) for w in args.workloads}
        fixtures["small"] = bench.build_fixture("small", "capture-repeat", 64)
        fixtures["dense-segmented"] = bench.build_fixture(
            "dense-segmented", "dense", roots["dense"], ("65536", str(args.segment_bytes)))
        bench.run_query(fixtures)
    if "growth" in args.experiments:
        bench.run_growth()
    print(f"raw measurements: {args.output / 'raw.jsonl'}")


if __name__ == "__main__":
    main()
