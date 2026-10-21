#!/usr/bin/env python3
"""Summarize tools/btel_query_bench.py output as Markdown tables.

Each cell is `median [min-max]` over repeats. Usage: btel_query_summary.py DIR
"""
from collections import defaultdict
import json
from pathlib import Path
import statistics
import sys

WORKLOAD_ORDER = ["tiny", "dense", "spawn", "capture-repeat", "capture-unique", "small",
                  "dense-segmented"]
VARIANTS = ["off-base", "off", "record-base", "record", "record+reader"]


def cell(values, scale=1.0, digits=1):
    values = [v * scale for v in values if v is not None]
    if not values:
        return "-"
    fmt = f"{{:.{digits}f}}"
    median = fmt.format(statistics.median(values))
    if len(values) == 1 or min(values) == max(values):
        return median
    return f"{median} [{fmt.format(min(values))}-{fmt.format(max(values))}]"


def table(headers, rows):
    lines = ["| " + " | ".join(headers) + " |", "|" + " --- |" * len(headers)]
    lines += ["| " + " | ".join(str(c) for c in row) + " |" for row in rows]
    return "\n".join(lines)


def order(names, preferred):
    return sorted(names, key=lambda n: (preferred.index(n) if n in preferred else 99, n))


def record_section(rows):
    by = defaultdict(list)
    for row in rows:
        by[(row["workload"], row["variant"])].append(row)
    out = ["## Recording (fixed work, fresh processes)", ""]
    headers = ["workload", "variant", "roots/s (k)", "vs off", "CPU incl. drain (s)",
               "drain (ms)", "peak RSS (MiB)", "recording (MB)", "CAS (MB)"]
    body = []
    for workload in order({w for w, _ in by}, WORKLOAD_ORDER):
        off = statistics.median(r["roots_per_s"] for r in by.get((workload, "off"), [])) \
            if by.get((workload, "off")) else None
        for variant in VARIANTS:
            runs = by.get((workload, variant))
            if not runs:
                continue
            rate = statistics.median(r["roots_per_s"] for r in runs)
            body.append([
                workload, variant, cell([r["roots_per_s"] for r in runs], 1e-3),
                f"{(rate / off - 1) * 100:+.1f}%" if off else "-",
                cell([r["cpu_including_drain_s"] for r in runs], digits=2),
                cell([r["drain_ms"] for r in runs]),
                cell([r["peak_rss_bytes"] for r in runs], 1 / 2**20, 0),
                cell([r.get("recording_bytes") for r in runs], 1e-6),
                cell([r.get("cas_bytes") for r in runs], 1e-6),
            ])
    out.append(table(headers, body))
    with_reader = [r for r in rows if r.get("reader")]
    if with_reader:
        out += ["", "Reader during recording (one `baml query` per second, fresh process; "
                "the final refresh runs after the recorder exits):", ""]
        body = []
        for workload in order({r["workload"] for r in with_reader}, WORKLOAD_ORDER):
            runs = [r for r in with_reader if r["workload"] == workload]
            live = [s for r in runs for s in r["reader"][:-1]]
            final = [r["reader"][-1] for r in runs]
            work = [s["refresh"] for s in live + final if s["refresh"]["files_decoded"]]
            rate = [w["bytes_read"] / (w["read_ms"] + w["apply_ms"] + w["commit_ms"]) / 1e3
                    for w in work]
            body.append([
                workload, len(live), cell([s["refresh"]["total_ms"] for s in live]),
                cell([s["refresh"]["files_decoded"] for s in live], digits=0),
                cell([s["refresh"]["total_ms"] for s in final]),
                cell([s["refresh"]["files_decoded"] for s in final], digits=0),
                cell(rate),
            ])
        out.append(table(["workload", "live refreshes", "live refresh (ms)",
                          "files per live refresh", "final refresh (ms)", "final files",
                          "indexing MB/s"], body))
    return out


def query_section(rows):
    out = ["## Queries (`baml query`, fresh process per query)", ""]
    sizes = {r["fixture"]: r for r in rows if r["phase"] == "size"}
    if sizes:
        body = []
        for fixture in order(sizes, WORKLOAD_ORDER):
            s = sizes[fixture]
            index = sum(s["index_files"].values())
            body.append([fixture, s["recordings"], s["recording_files"],
                         f"{s['recording_bytes'] / 1e6:.1f}", s["cas_blobs"],
                         f"{s['cas_bytes'] / 1e6:.1f}", f"{index / 1e6:.1f}",
                         f"{index / max(1, s['recording_bytes']):.2f}",
                         ", ".join(f"{k}={v / 1e6:.1f}" for k, v in sorted(s["index_files"].items()))])
        out += ["Fixtures and index size:", ""]
        out.append(table(["fixture", "recordings", "files", "recording MB", "CAS blobs",
                          "CAS MB", "index MB", "index/recording", "index files (MB)"], body))
        out.append("")
    checks = [r for r in rows if r["phase"] == "eviction-check"]
    if checks:
        out.append("Page-cache residency before/after eviction (bytes): " + "; ".join(
            f"{r['fixture']} {r['resident_before']}->{r['resident_after']}" for r in checks))
        out.append("")
    by = defaultdict(list)
    for row in rows:
        if row["phase"].startswith(("first-", "unchanged-")):
            by[(row["fixture"], row["query"], row["phase"])].append(row)
    headers = ["fixture", "query", "phase", "wall (ms)", "refresh (ms)", "read+apply+commit (ms)",
               "files decoded", "SQL (ms)", "CAS loads", "values", "peak RSS (MiB)",
               "storage read (MB)", "written (MB)", "status"]
    body = []
    for fixture in order({f for f, _, _ in by}, WORKLOAD_ORDER):
        for query in [q for q in dict.fromkeys(q for f, q, _ in by if f == fixture)]:
            for phase in ["first-warm", "first-evicted", "unchanged-warm", "unchanged-evicted"]:
                runs = by.get((fixture, query, phase))
                if not runs:
                    continue
                body.append([
                    fixture, query, phase, cell([r["wall_ms"] for r in runs]),
                    cell([r["refresh"]["total_ms"] for r in runs]),
                    cell([r["refresh"]["read_ms"] + r["refresh"]["apply_ms"] + r["refresh"]["commit_ms"]
                          for r in runs]),
                    cell([r["refresh"]["files_decoded"] for r in runs], digits=0),
                    cell([r["metrics"]["sql_ms"] for r in runs], digits=2),
                    cell([r["metrics"]["cas_loads"] for r in runs], digits=0),
                    cell([r["metrics"]["value_evaluations"] for r in runs], digits=0),
                    cell([r["peak_rss_bytes"] for r in runs], 1 / 2**20, 0),
                    cell([r["storage_read_bytes"] for r in runs], 1e-6),
                    cell([r["written_bytes"] for r in runs], 1e-6),
                    "/".join(sorted({r["status"] for r in runs})),
                ])
    out.append(table(headers, body))
    same = [r for r in rows if r["phase"].startswith("same-process")]
    if same:
        out += ["", "## Same process (one reused `Index`, as the playground)", ""]
        body = []
        for r in sorted(same, key=lambda r: (order([r["fixture"]], WORKLOAD_ORDER), r["phase"])):
            for q in r["queries"]:
                body.append([
                    r["fixture"], r["phase"].removeprefix("same-process-"),
                    f"{r['first_refresh']['total_ms']:.1f}", q["sql"][:48],
                    f"{q['first']['sql_ms']:.2f}",
                    cell([x["wall_ms"] for x in q["repeated"]], digits=2),
                    cell([x["refresh_ms"] for x in q["repeated"]], digits=2),
                    cell([x["query"]["cas_loads"] for x in q["repeated"]], digits=0),
                    f"{(r['peak_rss_bytes'] or 0) / 2**20:.0f}",
                ])
        out.append(table(["fixture", "index at open", "first refresh (ms)", "query",
                          "first SQL (ms)", "repeat refresh+query (ms)", "repeat refresh (ms)",
                          "repeat CAS loads", "peak RSS (MiB)"], body))
    return out


def growth_section(rows):
    out = ["## Growth (fixed addition to increasing history, fresh process)", ""]
    levels = sorted({r["bulks"] for r in rows})
    body = []
    for level in levels:
        inc = [r for r in rows if r["bulks"] == level and r["phase"] == "incremental"]
        rebuild = [r for r in rows if r["bulks"] == level and r["phase"] == "rebuild"]
        size = next((r for r in rows if r["bulks"] == level and r["phase"] == "size"), None)
        if not inc:
            continue
        history = inc[0]["history"]
        body.append([
            level, history["recording_files"], f"{history['recording_bytes'] / 1e6:.1f}",
            cell([r["refresh"]["files_decoded"] for r in inc], digits=0),
            cell([r["refresh"]["total_ms"] for r in inc]),
            cell([r["refresh"]["discovery_ms"] for r in inc]),
            cell([r["wall_ms"] for r in inc]),
            cell([r["written_bytes"] for r in inc], 1e-6),
            cell([r["refresh"]["total_ms"] for r in rebuild]),
            cell([r["wall_ms"] for r in rebuild]),
            f"{sum(size['index_files'].values()) / 1e6:.1f}" if size else "-",
        ])
    out.append(table(["bulk recordings", "history files", "history MB", "files decoded",
                      "incremental refresh (ms)", "discovery (ms)", "incremental wall (ms)",
                      "written (MB)", "rebuild refresh (ms)", "rebuild wall (ms)", "index MB"],
                     body))
    return out


def main():
    directory = Path(sys.argv[1])
    rows = [json.loads(line) for line in (directory / "raw.jsonl").read_text().splitlines()]
    env = json.loads((directory / "environment.json").read_text())
    out = [f"Environment: {env['cpu']}, {env['cpu_count']} CPUs, {env['platform']}; "
           f"storage {env['filesystem']}; head {env['git_head']}", ""]
    sections = [
        ([r for r in rows if r["experiment"] == "record"], record_section),
        ([r for r in rows if r["experiment"] == "query"], query_section),
        ([r for r in rows if r["experiment"] == "growth"], growth_section),
    ]
    for selected, render in sections:
        if selected:
            out += render(selected) + [""]
    print("\n".join(out))


if __name__ == "__main__":
    main()
