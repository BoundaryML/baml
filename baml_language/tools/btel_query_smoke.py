#!/usr/bin/env python3
"""Short reader benchmark. Build binaries and prepare fixtures once; then run.

Run never records, builds Rust, evicts the OS page cache, or changes the source
fixtures. Each run indexes a private scratch copy on the fixtures' filesystem.
The first index has no SQLite cache, but its BTEL bytes may be OS-cached.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import tempfile
import time

LANGUAGE = Path(__file__).resolve().parents[1]
FIXTURES = {"tiny": 10000, "dense": 128, "spawn": 256,
            "capture-repeat": 10000, "capture-unique": 512}
QUERIES = {
    "count": "SELECT count(*) FROM calls",
    "executions": "SELECT * FROM executions ORDER BY started_at_ms DESC, execution_id LIMIT 20",
    "function_stats": "SELECT fqn, sum(call_count) AS calls, sum(total_duration_ns) AS ns "
                      "FROM function_stats GROUP BY fqn ORDER BY calls DESC, fqn",
    "call_path_stats": "SELECT * FROM call_path_stats ORDER BY self_ns DESC, call_path_id LIMIT 20",
    "hot_call_paths": "SELECT * FROM hot_call_paths ORDER BY self_ns DESC, call_path_id LIMIT 20",
    "cas_filter": "SELECT count(*) FROM calls WHERE args['n'] = 7",
}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def fingerprint(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def check(condition, message):
    if not condition:
        raise RuntimeError(message)


def invoke(command, timeout, env=None):
    started = time.perf_counter()
    child = subprocess.run([str(c) for c in command], capture_output=True, text=True,
                           env=env, timeout=timeout, check=False)
    check(child.returncode == 0,
          f"Command exited {child.returncode}: {command}\n{child.stderr}\n{child.stdout}")
    return json.loads(child.stdout), (time.perf_counter() - started) * 1000


def inventory(project):
    root = project / ".baml/btel"
    return [{"path": str(p.relative_to(project)), "bytes": p.stat().st_size,
             "sha256": digest(p)} for folder in ("recordings", "cas")
            for p in sorted((root / folder).rglob("*")) if p.is_file()]


def prepare(args):
    check(not args.fixtures.exists(), "Fixture directory already exists; reuse it with run, "
          "or prepare a different directory.")
    args.fixtures.mkdir(parents=True)
    started = time.perf_counter()
    manifest = {"version": 1, "producer": str(args.recorder),
                "producer_sha256": digest(args.recorder), "fixtures": {}}
    for name, roots in FIXTURES.items():
        print(f"recording {name}: {roots} roots", flush=True)
        result, _ = invoke([args.recorder, "btel", name, roots, args.fixtures / name,
                            4096, 16384], 60, dict(os.environ, BAML_TELEMETRY="medium"))
        files = inventory(args.fixtures / name)
        check(sum(f["path"].endswith(".btel") for f in files) >= 2,
              f"{name}: need at least two segments for the incremental measurement")
        manifest["fixtures"][name] = {"recording": result, "files": files}
    manifest["prepare_seconds"] = time.perf_counter() - started
    (args.fixtures / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Fixtures ready in {manifest['prepare_seconds']:.1f}s: {args.fixtures}")


def link_or_copy(source, target):
    target.parent.mkdir(parents=True, exist_ok=True)
    try:
        os.link(source, target)
    except OSError:
        shutil.copy2(source, target)


class Run:
    def __init__(self, args):
        self.args = args
        self.started = time.perf_counter()
        self.deadline = time.monotonic() + args.budget_seconds
        self.records = []

    def timeout(self):
        remaining = self.deadline - time.monotonic()
        check(remaining > 0, "Benchmark exhausted its time budget")
        return min(45, remaining)

    def query(self, project, fixture, phase, name, *, unchanged=False):
        result, wall = invoke([self.args.baml, "query", "--from", project, "--format", "json",
                               "--max-wall", "40s", QUERIES[name]], self.timeout(),
                              dict(os.environ, BAML_AGENT_SKILL_CHECK="off"))
        outcome = result["outcome"]
        check(outcome["status"] == "complete", f"Incomplete result: {outcome}")
        refresh = outcome["refresh"]
        if unchanged:
            check(refresh["files_decoded"] == 0 and refresh["bytes_read"] == 0
                  and refresh["transactions"] == 0, f"Unchanged source did work: {refresh}")
        if name != "cas_filter":
            check(outcome["query"]["cas_loads"] == 0, "Metadata query read CAS")
        row = {"fixture": fixture, "phase": phase, "query": name, "wall_ms": wall,
               "refresh": refresh, "metrics": outcome["query"],
               "result_sha256": fingerprint(result["rows"]), "rows": len(result["rows"])}
        if name in ("count", "cas_filter"):
            row["count"] = result["rows"][0][0]
        self.records.append(row)
        return row

    def fixture(self, name, spec, project):
        source = self.args.fixtures / name
        files = spec["files"]
        recordings = [f for f in files if f["path"].endswith(".btel")]
        # Withhold just the final segment: the initial source is a valid prefix.
        addition = max(recordings, key=lambda f: f["path"])
        for entry in files:
            path = source / entry["path"]
            check(path.stat().st_size == entry["bytes"] and digest(path) == entry["sha256"],
                  f"Fixture changed: {path}; prepare a new fixture set")
            if entry != addition:
                link_or_copy(path, project / entry["path"])
        first = self.query(project, name, "first_index", "count")
        check(first["refresh"]["files_decoded"] == len(recordings) - 1,
              "First query did not decode the expected prefix")
        link_or_copy(source / addition["path"], project / addition["path"])
        extra = self.query(project, name, "one_new_file", "count")
        check(extra["refresh"]["files_decoded"] == 1
              and extra["refresh"]["files_applied"] == 1
              and extra["refresh"]["bytes_read"] == addition["bytes"],
              f"Incremental refresh reread history: {extra['refresh']}")
        expected_calls = FIXTURES[name] if name.startswith("capture-") else 0
        check(extra["count"] == expected_calls, f"Unexpected retained call count in {name}")
        names = [q for q in self.args.queries if q != "cas_filter" or name.startswith("capture-")]
        expected = {}
        for query in names:
            # Prime query-specific pages once, outside measured repetitions.
            warmup = self.query(project, name, "warmup", query, unchanged=True)
            expected[query] = warmup["result_sha256"]
            for _ in range(self.args.repeats):
                row = self.query(project, name, "cli_warm", query, unchanged=True)
                check(row["result_sha256"] == expected[query], "Repeated query changed rows")
        if "cas_filter" in names:
            cas = next(r for r in self.records if r["fixture"] == name and r["query"] == "cas_filter")
            expected_matches = 1 if name == "capture-unique" else FIXTURES[name]
            expected_loads = FIXTURES[name] if name == "capture-unique" else 1
            check(cas["count"] == expected_matches and cas["metrics"]["cas_loads"] == expected_loads,
                  "Captured-value workload did not evaluate real captures")
        if not names:
            return
        result, _ = invoke([self.args.query_bench, project, self.args.repeats,
                             *(QUERIES[q] for q in names)], self.timeout())
        check(result["first_refresh"]["bytes_read"] == 0, "Reused connection reread source")
        for query, measured in zip(names, result["queries"], strict=True):
            check("result_rows" in measured, "query_bench binary is stale; rebuild the example")
            check(measured["status"] == "complete", "Reused connection returned incomplete evidence")
            check(fingerprint(measured["result_rows"]) == expected[query],
                  f"CLI and reused connection disagree: {name}/{query}")
            for row in measured["repeated"]:
                if query != "cas_filter":
                    check(row["query"]["cas_loads"] == 0, "Metadata query read CAS")
                self.records.append({"fixture": name, "phase": "connection_warm", "query": query,
                                     "wall_ms": row["wall_ms"], "refresh_ms": row["refresh_ms"],
                                     "metrics": row["query"], "result_sha256": expected[query]})
        print(f"{name}: first index {first['refresh']['total_ms']:.1f}ms, "
              f"one new file {extra['refresh']['total_ms']:.1f}ms", flush=True)


def summary(records):
    result = []
    for key in dict.fromkeys((r["fixture"], r["phase"], r["query"]) for r in records):
        if key[1] == "warmup":
            continue
        rows = [r for r in records if (r["fixture"], r["phase"], r["query"]) == key]
        wall = [r["wall_ms"] for r in rows]
        result.append({"fixture": key[0], "phase": key[1], "query": key[2],
                       "median_ms": statistics.median(wall), "min_ms": min(wall), "max_ms": max(wall),
                       "refresh_ms": statistics.median(r.get("refresh", {}).get("total_ms", r.get("refresh_ms", 0)) for r in rows),
                       "btel_bytes": rows[0].get("refresh", {}).get("bytes_read", 0),
                       "cas_loads": rows[0]["metrics"]["cas_loads"],
                       "cas_bytes": rows[0]["metrics"]["cas_bytes_read"],
                       "result_sha256": rows[0]["result_sha256"]})
    return result


def run(args):
    check(not args.output.exists(), "Output exists; choose a new report path")
    manifest = json.loads((args.fixtures / "manifest.json").read_text())
    check(manifest["version"] == 1, "Unsupported fixture manifest")
    check(set(manifest["fixtures"]) == set(FIXTURES),
          "Fixture set is stale; prepare the current workloads in a new directory")
    git_head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=LANGUAGE, text=True).strip()
    git_diff = subprocess.check_output(["git", "diff", "--stat"], cwd=LANGUAGE, text=True).strip()
    load_start = os.getloadavg()
    bench = Run(args)
    failure = None
    try:
        # Keep scratch on the same disk as fixtures, never a possibly-tmpfs /tmp.
        with tempfile.TemporaryDirectory(prefix="reader-smoke-", dir=args.fixtures.parent) as scratch:
            for name in args.workloads:
                print(f"measuring {name} ({time.perf_counter() - bench.started:.1f}s elapsed)", flush=True)
                bench.fixture(name, manifest["fixtures"][name], Path(scratch) / name)
        bench.timeout()
    except (Exception, KeyboardInterrupt) as error:
        # Keep completed samples on a timeout or failed invariant; never
        # represent a partial run as a successful performance comparison.
        failure = error
    report = {"version": 1, "elapsed_seconds": time.perf_counter() - bench.started,
              "fixture_sha256": fingerprint(manifest), "platform": platform.platform(),
              "git_head": git_head, "git_diff_stat": git_diff,
              "load_start": load_start, "load_end": os.getloadavg(), "repeats": args.repeats,
              "workloads": args.workloads, "queries": args.queries,
              "sql": {name: QUERIES[name] for name in args.queries},
              "source": {name: manifest["fixtures"][name]["recording"] for name in args.workloads},
              "cache": "warm OS cache; fresh private SQLite index per fixture",
              "binaries": {name: {"path": str(path), "sha256": digest(path)}
                           for name, path in (("baml", args.baml), ("query_bench", args.query_bench))},
              "records": bench.records, "summary": summary(bench.records)}
    if args.compare and failure is None:
        try:
            before = json.loads(args.compare.read_text())
            check(before["status"] == "complete", "Cannot compare with a failed or partial run")
            check(before["fixture_sha256"] == report["fixture_sha256"], "Comparison needs identical fixtures")
            check(all(before["sql"].get(name) == sql for name, sql in report["sql"].items()),
                  "Comparison needs identical SQL statements")
            old = {(r["fixture"], r["phase"], r["query"]): r for r in before["summary"]}
            for row in report["summary"]:
                previous = old[(row["fixture"], row["phase"], row["query"])]
                check(previous["result_sha256"] == row["result_sha256"],
                      f"Query rows changed: {row['fixture']}/{row['phase']}/{row['query']}")
                row["before_ms"] = previous["median_ms"]
                row["speedup"] = previous["median_ms"] / row["median_ms"]
        except Exception as error:
            failure = error
    report["status"] = "failed" if failure is not None else "complete"
    if failure is not None:
        report["failure"] = f"{type(failure).__name__}: {failure}"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as output:
        output.write(json.dumps(report, indent=2) + "\n")
    if failure is not None:
        raise RuntimeError(f"{report['failure']}\nPartial results: {args.output}") from failure
    print("\nfixture         phase             query               median ms   refresh ms   BTEL bytes   CAS loads/bytes")
    for row in report["summary"]:
        ratio = f"  {row['speedup']:.2f}x" if "speedup" in row else ""
        print(f"{row['fixture']:<15} {row['phase']:<17} {row['query']:<19} {row['median_ms']:9.2f} "
              f"{row['refresh_ms']:12.2f} {row['btel_bytes']:12} {row['cas_loads']:6}/{row['cas_bytes']}{ratio}")
    print(f"\nCompleted in {report['elapsed_seconds']:.1f}s; report: {args.output}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare", help="record a small fixed corpus once")
    prep.add_argument("--recorder", type=Path, default=LANGUAGE / "target/release/examples/btel_record_bench")
    measure = commands.add_parser("run", help="measure only the reader (no builds or recording)")
    measure.add_argument("--baml", type=Path, default=LANGUAGE / "target/release/baml-cli")
    measure.add_argument("--query-bench", type=Path, default=LANGUAGE / "target/release/examples/query_bench")
    measure.add_argument("--output", type=Path, required=True)
    measure.add_argument("--compare", type=Path, help="previous JSON report; also verifies unchanged query rows")
    measure.add_argument("--repeats", type=int, default=3)
    measure.add_argument("--workloads", nargs="+", choices=list(FIXTURES), default=list(FIXTURES))
    measure.add_argument("--queries", nargs="+", choices=list(QUERIES), default=list(QUERIES))
    measure.add_argument("--budget-seconds", type=float, default=120, help="whole-run wall budget")
    for command in (prep, measure):
        command.add_argument("--fixtures", type=Path, default=LANGUAGE / "target/btel-query-smoke")
    args = parser.parse_args()
    for name, value in vars(args).items():
        if isinstance(value, Path):
            setattr(args, name, value.resolve())
    if args.command == "run" and (args.repeats < 1 or args.budget_seconds <= 0):
        parser.error("repeats and budget-seconds must be positive")
    binaries = ("baml", "query_bench") if args.command == "run" else ("recorder",)
    for name in binaries:
        binary = getattr(args, name)
        if not binary.is_file() or not os.access(binary, os.X_OK):
            parser.error(f"Missing executable {binary}; build the release binaries first")
    if args.command == "run" and not (args.fixtures / "manifest.json").is_file():
        parser.error("Fixtures are missing or incomplete; run prepare in a new directory first")
    try:
        (prepare if args.command == "prepare" else run)(args)
    except (RuntimeError, subprocess.TimeoutExpired) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
