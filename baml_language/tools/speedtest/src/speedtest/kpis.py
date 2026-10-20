"""The KPI report: kpis.toml + one run's summary -> kpis.json -> kpis.html.

Every KPI chart draws every row of the run. A row with no value says why (not
installed, failed verification, does not apply) instead of being dropped, so
a missing comparison is visible.
"""

import json
import tomllib
from pathlib import Path

from speedtest.loader import ROOT
from speedtest.rows import PRIMARY

KPI_FILE = ROOT / "kpis.toml"
TEMPLATE = Path(__file__).resolve().parent / "templates" / "kpis.html"
UNITS = {"throughput": ("jobs/s", "higher"), "process_ms": ("ms", "lower"), "peak_rss_mib": ("MiB", "lower"),
         "binary_kib": ("KB", "lower"), "build_s": ("s", "lower")}
NO_BINARY = {"note": "no binary: ships source and a runtime"}
NO_BUILD = {"note": "no build step"}
SAME_AS_OFF = {"note": "same executable as telemetry off"}


def load():
    with open(KPI_FILE, "rb") as f:
        defs = tomllib.load(f)["kpi"]
    for k in defs:
        if k["metric"] not in UNITS:
            raise SystemExit(f"kpis.toml: {k['key']}: unknown metric {k['metric']!r}")
    return defs


def workload_names():
    return {name for k in load() for name in k["workloads"]}


def _scaled(stats, f):
    return {"median": stats["median"] * f, "min": stats["min"] * f, "max": stats["max"] * f, "n": stats["n"]}


def _value(metric, cell, build, row):
    """One bar: a measured value, a note saying why there is none, or None (row-level reason)."""
    if metric == "binary_kib":
        if not row.is_baml:
            return NO_BINARY
        if row.telemetry != "off":
            return SAME_AS_OFF
        return {"median": build["bytes"] / 1024} if build.get("status") == "ok" else {"note": "pack failed"}
    if metric == "build_s":
        if not row.is_baml:
            return NO_BUILD
        if row.telemetry != "off":
            return SAME_AS_OFF
        return {"median": build["seconds"]} if build.get("status") == "ok" else {"note": "pack failed"}
    if cell["status"] == "failed":
        return {"note": "failed: " + (cell.get("reason") or "?")[:120]}
    if cell["status"] != "ok" or not cell.get("per_job_ns"):
        return None
    if metric == "throughput":
        pj = cell["per_job_ns"]
        # jobs/s from per-job ns; min and max swap because faster is more
        return {"median": 1e9 / pj["median"], "min": 1e9 / pj["max"], "max": 1e9 / pj["min"], "n": pj["n"],
                "us_per_job": pj["median"] / 1e3}
    if metric == "process_ms":
        return _scaled(cell["process_ns"], 1e-6)
    return _scaled(cell["peak_rss_bytes"], 1 / 2**20)


def build(data, workloads, row_list):
    by_name = {w["name"]: w for w in workloads}
    summary, builds = data["summary"], data["builds"]
    present = [r for r in row_list]
    refs = [r.key for r in present if not r.is_baml]
    out_kpis = []
    for k in load():
        unit, better = UNITS[k["metric"]]
        scenarios = []
        for name in k["workloads"]:
            w = by_name.get(name)
            if not w:
                continue
            cells, b = summary[w["id"]], builds.get(w["id"], {})
            values = {r.key: _value(k["metric"], cells[r.key], b.get(r.backend, {}), r) for r in present}
            p = values.get(PRIMARY) or {}
            headline = {}
            for ref in refs:
                v = values.get(ref) or {}
                headline[ref] = p["median"] / v["median"] if p.get("median") and v.get("median") else None
            scenarios.append({"name": name, "label": f"{name.split('::', 1)[1]}: {w['description']}", "values": values,
                              "headline": headline, "source": w["baml"]})
        out_kpis.append({"key": k["key"], "title": k["title"], "question": k["question"], "measure": k["measure"],
                         "unit": unit, "better": better, "default": scenarios[0]["name"] if scenarios else None,
                         "scenarios": scenarios, **({} if scenarios else {"pending": "none of its workloads ran in this run"})})

    rows = []
    for r in present:
        statuses = {summary[w["id"]][r.key]["status"] for w in workloads}
        status = "measured" if "ok" in statuses else "unavailable" if statuses == {"unavailable"} else "failed"
        rows.append({"key": r.key, "label": r.label, "family": r.family, "status": status})

    all_rows = []
    for w in workloads:
        cells = summary[w["id"]]
        all_rows.append({"name": w["name"], "description": w["description"], "origin": w.get("origin"),
                         "values": {r.key: (cells[r.key]["per_job_ns"]["median"] if cells[r.key].get("per_job_ns") else None)
                                    for r in present},
                         "status": {r.key: cells[r.key]["status"] for r in present}})

    vcs = (data.get("cli") or {}).get("vcs") or {}
    trials_total = sum(s["trials"] for ws in summary.values() for s in ws.values())
    trials_failed = sum(s["failed_trials"] for ws in summary.values() for s in ws.values())
    verify_failures = sum(1 for ws in summary.values() for s in ws.values() if s["status"] == "failed")
    return {
        "schema": 3,
        "primary": PRIMARY,
        "compare": [r for r in ("node", "python", "bun") if r in refs][:2],
        "candidate": {"sha": vcs.get("commit"), "branch": vcs.get("branch") or vcs.get("bookmark"),
                      "subject": vcs.get("message"), "cli": (data.get("cli") or {}).get("version")},
        "machine": {**data.get("machine", {}), "toolchains": data.get("toolchains", {})},
        "protocol": {"name": data.get("protocol"), "trials": data.get("trials"), "seed": data.get("seed"), "cpu": data.get("cpu")},
        "run": {"id": data.get("id"), "started": data.get("started"), "finished": data.get("finished"),
                "trials_total": trials_total, "trials_failed": trials_failed, "verify_failures": verify_failures,
                "loadavg_start": data.get("loadavg_start"), "loadavg_end": data.get("loadavg_end")},
        "rows": rows,
        "reasons": {"unavailable": "the toolchain is not installed on this machine",
                    "failed": "failed verification or every trial on this workload"},
        "kpis": out_kpis,
        "workloads": all_rows,
    }


def cmd_report(args):
    """Re-render a saved run's report from its meta.json, with the current kpis.toml and template."""
    from speedtest.loader import load_workloads
    from speedtest.rows import BY_KEY
    from speedtest.storage import resolve_ref, results_dir
    path = Path(args.run)
    if not (path / "meta.json").exists():
        path = resolve_ref(results_dir(args), args.run)
    if not path or not (Path(path) / "meta.json").exists():
        raise SystemExit(f"no run at {args.run}")
    data = json.loads((Path(path) / "meta.json").read_text())
    if "summary" not in data:
        raise SystemExit(f"{path} predates the KPI report; re-run it")
    by_id = {w["id"]: w for w in load_workloads()}
    workloads = [by_id[i] for i in data["summary"] if i in by_id]
    print(render(path, data, workloads, [BY_KEY[k] for k in data["rows"]]))


def render(run_path, data, workloads, row_list):
    report = build(data, workloads, row_list)
    run_path = Path(run_path)
    (run_path / "kpis.json").write_text(json.dumps(report, indent=1))
    page = TEMPLATE.read_text().replace("/*__KPIS__*/null", json.dumps(report).replace("</", "<\\/"))
    (run_path / "kpis.html").write_text(page)
    return run_path / "kpis.html"
