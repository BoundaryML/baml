"""The KPI data model: one kpis.json per bundle, rendered by templates/kpis.html.

    python3 -m bench render <bundle> [--previous <bundle>]   -> <bundle>/kpis.json, <bundle>/kpis.html

Each KPI is a question a user asks of a language, answered by one chart in
which every implementation is a bar: BAML native and BAML VM at every
telemetry level (unsupported levels are drawn as "not yet", never omitted),
then Rust, Go, Node, Bun and Python. Execution KPIs are throughput, higher
is better, as a user reads a benchmark; the per-job time is in the table.
"""
import json
import statistics
from pathlib import Path

from .report import Bundle

ROOT = Path(__file__).resolve().parents[1]
FAMILIES = [("BAML native", ["native-off", "native-low", "native-medium", "native-high"]),
            ("BAML VM", ["vm-off", "vm-low", "vm-medium", "vm-high"]),
            ("references", ["rust", "go", "node", "bun", "python"])]
LABELS = {"native-off": "native · telemetry off", "native-low": "native · low", "native-medium": "native · medium", "native-high": "native · high",
          "vm-off": "VM · telemetry off", "vm-low": "VM · low", "vm-medium": "VM · medium", "vm-high": "VM · high",
          "rust": "Rust", "go": "Go", "node": "Node", "bun": "Bun", "python": "Python", "control": "control"}
UNSUPPORTED = {"native-low", "native-medium", "native-high"}
SCENARIO_LABELS = {
    "startup": "process start to ready", "array-empty": "call with an empty array", "array-small": "sum 128 integers",
    "array-iterator": "iterate 8,192 integers", "array-indexed": "index 8,192 integers", "array-build": "build and sum 8,192 integers",
    "merge-random": "merge sort 1,024 random integers", "merge-sorted": "merge sort 1,024 sorted integers", "merge-reverse": "merge sort 1,024 reversed integers",
    "merge-duplicates": "merge sort 1,024 integers, many duplicates", "quick-random": "quick sort 1,024 random integers", "quick-sorted": "quick sort 1,024 sorted integers",
    "quick-reverse": "quick sort 1,024 reversed integers", "quick-duplicates": "quick sort 1,024 integers, many duplicates",
    "json-aggregate": "parse, validate and aggregate 1,024 JSON rows", "json-hello": "serialize a record to JSON",
    "allocation-low-retention": "allocate 128 batches of 2,048 objects, keep 4", "allocation-high-retention": "allocate 128 batches of 2,048 objects, keep 64",
    "calls-empty": "call a function 0 times", "calls": "call a leaf function 128 times", "generate-sort": "generate and sort 8,192 numbers with the library sort"}
# One plain sentence per KPI on what the timed region contains.
MEASURE = {
    "sort": "One job copies the input, sorts it in BAML code (no library sort) and serializes the whole result to JSON; the bar is jobs per second.",
    "arrays": "One job walks or builds an array of 8,192 integers in BAML code and returns the sum; the bar is jobs per second.",
    "json": "One job parses a JSON document, validates every row, groups and sorts, and serializes the answer; the bar is jobs per second.",
    "alloc": "One job allocates 128 batches of 2,048 small objects, keeping 4 or 64 batches alive, so the collector or the reference counts do real work; the bar is jobs per second.",
    "calls": "One job calls a one-line leaf function 128 times in a loop; the bar is jobs per second, so it is the cost of a call.",
    "startup": "Time from the parent launching the process to the process reporting ready, after loading its image and preparing state; milliseconds, lower is better.",
    "memory": "Peak resident memory of the whole process over the run, from the kernel's wait4 accounting; MiB, lower is better.",
    "artifact": "Size of the executable a user would ship, stripped; BAML VM counts the bytecode plus the measurement host it needs; KB, lower is better.",
    "build": "Wall time of a clean release build on the measuring machine; seconds, lower is better.",
    "http": "Requests per second and p99 latency of a small HTTP service under a fixed arrival rate.",
}
# Why a row can be empty, shown on the chart and in the footer.
REASONS = {
    "unsupported": "native telemetry does not exist yet; it arrives with the telemetry PR, and these rows fill in then",
    "not_run": "added to the protocol after this run; present from the next run",
    "pending_http": "needs sys ops compiled natively (PR 8); the VM's HTTP harness has not been run on this machine yet",
}
KPIS = [
    ("sort", "Sorting", "How fast does it sort?", ["merge-random", "quick-random", "merge-sorted", "merge-reverse", "merge-duplicates", "quick-sorted", "quick-reverse", "quick-duplicates"]),
    ("arrays", "Arrays and loops", "How fast does it iterate, index and build arrays?", ["array-iterator", "array-indexed", "array-build", "array-small", "array-empty"]),
    ("json", "JSON", "How fast does it parse, validate and serialize JSON?", ["json-aggregate", "json-hello"]),
    ("alloc", "Allocation", "How fast does it allocate and reclaim many small objects?", ["allocation-high-retention", "allocation-low-retention", "generate-sort"]),
    ("calls", "Function calls", "What does a function call cost?", ["calls", "calls-empty"]),
]


def rows_status(b):
    out = []
    for family, keys in FAMILIES:
        for k in keys:
            measured = any(b.cell(s, k) for s in b.scenarios())
            status = "unsupported" if k in UNSUPPORTED else ("measured" if measured else "not_run")
            out.append({"key": k, "label": LABELS[k], "family": family, "status": status})
    return out


def val(b, scen, row, metric, f, per_job):
    c = b.cell(scen, row)
    m = c and c.get(metric)
    if not m:
        return None
    src = m.get("per_job", m) if per_job else m
    return {"median": src["median"] * f, "min": src["min"] * f, "max": src["max"] * f, "n": c.get("n")}


def throughput(b, scen, row):
    v = val(b, scen, row, "execution_wall_ns", 1, True)
    if not v:
        return None
    # jobs per second from per-job wall ns; min/max swap because faster is more
    return {"median": 1e9 / v["median"], "min": 1e9 / v["max"], "max": 1e9 / v["min"], "n": v["n"], "us_per_job": v["median"] / 1e3}


def scenario_entry(b, scen, value_fn):
    case = b.case_of(scen)
    src = ROOT / "cases" / case / "workload.baml"
    return {"name": scen, "label": SCENARIO_LABELS.get(scen, scen), "case": case,
            "values": {k: value_fn(b, scen, k) for _f, keys in FAMILIES for k in keys},
            "source": src.read_text() if src.exists() else None}


def headline(entry, vs=("node", "go")):
    nat = entry["values"].get("native-off")
    out = {}
    for r in vs:
        v = entry["values"].get(r)
        out[r] = nat["median"] / v["median"] if nat and v and v["median"] else None
    return out


def cpu_text(m):
    """The CPU as a reader would name it: /proc/cpuinfo's model name, or the machine file's description
    when the guest only reports a family/model number (KVM guests do)."""
    model = (m.get("cpu_model") or "").split(":")[-1].strip()
    if any(ch.isalpha() for ch in model):
        return model
    mf = ROOT / "machines" / f"{m.get('machine')}.json"
    if mf.exists():
        mj = json.loads(mf.read_text())
        return mj.get("cpu") or mj.get("description", model)
    return model


def build(bundle_dir, previous=None):
    b = Bundle(bundle_dir)
    m = b.manifest
    st = b.summary.get("startup", {})
    builds = m.get("builds", {})
    arts = m.get("artifacts", {})
    kpis = []
    for key, title, question, scens in KPIS:
        entries = [scenario_entry(b, s, throughput) for s in scens if s in b.summary]
        for e in entries:
            e["headline"] = headline(e)
        kpis.append({"key": key, "title": title, "question": question, "measure": MEASURE[key], "unit": "ops/s", "better": "higher",
                     "default": entries[0]["name"] if entries else None, "scenarios": entries})
    # startup: launch to ready, lower is better
    e = scenario_entry(b, "startup", lambda b_, s, r: val(b_, s, r, "launch_to_ready_ns", 1e-6, False))
    e["label"] = "process start to ready, launch observed by the parent"; e["headline"] = {r: (e["values"]["native-off"]["median"] / e["values"][r]["median"]) if e["values"].get(r) and e["values"].get("native-off") else None for r in ("node", "go")}
    kpis.append({"key": "startup", "title": "Startup", "question": "How fast does a fresh process get to work?", "measure": MEASURE["startup"], "unit": "ms", "better": "lower", "default": "startup", "scenarios": [e]})
    # memory: peak RSS per scenario
    mem = [scenario_entry(b, s, lambda b_, s_, r: val(b_, s_, r, "peak_rss_bytes", 1 / 2**20, False)) for s in ["allocation-high-retention", "allocation-low-retention", "json-aggregate", "merge-random", "startup"] if s in b.summary]
    for e in mem:
        e["headline"] = {r: (e["values"]["native-off"]["median"] / e["values"][r]["median"]) if e["values"].get(r) and e["values"].get("native-off") else None for r in ("node", "go")}
    kpis.append({"key": "memory", "title": "Memory", "question": "How much memory does a process use at its peak?", "measure": MEASURE["memory"], "unit": "MiB", "better": "lower", "default": mem[0]["name"] if mem else None, "scenarios": mem})
    # artifact size per case: native stripped, VM bytecode + host, rust, go. Rows that do
    # not apply say why instead of standing empty (a note is drawn in italics, never a bar).
    host_bytes = (m.get("host") or {}).get("bytes")
    same_art = {"note": "same artifact as telemetry off"}
    art = []
    for case, bd in builds.items():
        if bd.get("status") != "ok":
            continue
        a = arts.get(case, {})
        vals = {"native-off": {"median": bd["stripped_bytes"] / 1024},
                "vm-off": {"median": (a["program"]["bytes"] + (host_bytes or 0)) / 1024} if "program" in a else None,
                "vm-low": same_art, "vm-medium": same_art, "vm-high": same_art,
                "rust": {"median": a["rust"]["bytes"] / 1024} if "rust" in a else None,
                "go": {"median": a["go"]["bytes"] / 1024} if "go" in a else None,
                "node": {"note": "no binary: ships source and a runtime"}, "bun": {"note": "no binary: ships source and a runtime"},
                "python": {"note": "no binary: ships source and a runtime"}}
        art.append({"name": case, "label": f"{case.split('_', 1)[1].replace('_', ' ')}: executable to ship", "case": case, "values": vals,
                    "headline": {"go": vals["native-off"]["median"] / vals["go"]["median"] if vals.get("go") else None, "node": None}})
    kpis.append({"key": "artifact", "title": "Binary size", "question": "How big is the thing you ship?", "measure": MEASURE["artifact"] + " The published packed CLI is 32.7 MiB.", "unit": "KB", "better": "lower", "default": art[0]["name"] if art else None, "scenarios": art})
    # build time: native clean build; bytecode compile where recorded
    same_build = {"note": "same compile as telemetry off"}
    bl = []
    for case, bd in builds.items():
        if bd.get("status") != "ok":
            continue
        vals = {"native-off": {"median": bd["seconds"]}, "vm-low": same_build, "vm-medium": same_build, "vm-high": same_build,
                "rust": {"note": "prebuilt into the snapshot"}, "go": {"note": "prebuilt into the snapshot"},
                "node": {"note": "no build step"}, "bun": {"note": "no build step"}, "python": {"note": "no build step"}}
        bc = (m.get("bytecode_compile_seconds") or {}).get(case)
        if bc:
            vals["vm-off"] = {"median": bc}
        bl.append({"name": case, "label": f"{case.split('_', 1)[1].replace('_', ' ')}: clean release build", "case": case, "values": vals, "headline": {}})
    kpis.append({"key": "build", "title": "Build time", "question": "How long from source to a runnable artifact?", "measure": MEASURE["build"] + " Native is a cargo build with fat LTO on 4 vCPU; VM is the bytecode compile by the pinned host.", "unit": "s", "better": "lower", "default": bl[0]["name"] if bl else None, "scenarios": bl})
    # web server: not measured on this machine yet
    kpis.append({"key": "http", "title": "Web server", "question": "How many requests per second does a small HTTP service handle?", "measure": MEASURE["http"], "unit": "req/s", "better": "higher", "default": None, "scenarios": [], "pending": REASONS["pending_http"]})
    data = {"schema": 2,
            "candidate": {"sha": m.get("candidate_sha"), "branch": m.get("candidate_branch"), "subject": m.get("candidate_subject"), "dirty_files": m.get("candidate_dirty_files")},
            "machine": {"name": m.get("machine"), "cpu": cpu_text(m), "kernel": m.get("kernel"), "glibc": (m.get("glibc") or "").split()[-1],
                        "rustc": next((bd["rustc"] for bd in builds.values() if bd.get("rustc")), (m.get("rustc") or "").splitlines()[0] if m.get("rustc") else None)},
            "protocol": {"name": m.get("protocol"), "trials": m.get("trials"), "cpu": m.get("cpu"), "workers": m.get("workers"), "sha256": m.get("protocol_sha256")},
            "run": {"started": m.get("started"), "finished": m.get("finished"), "trials_total": m.get("trials_total"), "trials_failed": m.get("trials_failed"),
                    "verify_failures": (m.get("verify") or {}).get("failures"), "loadavg_start": m.get("loadavg_start"), "loadavg_end": m.get("loadavg_end")},
            "rows": rows_status(b), "reasons": REASONS, "kpis": kpis,
            "drift": [{"scenario": s, "ratio": b.us(s, "native-off") / b.us(s, "control")} for s in b.scenarios() if b.us(s, "control") and b.us(s, "native-off")]}
    if previous:
        from .report import verdicts
        p = Bundle(previous)
        data["previous"] = {"sha": p.manifest.get("candidate_sha"), "verdicts": verdicts(b, p, b.protocol.get("regression", {}))}
    return data


def render(bundle_dir, previous=None, out=None):
    data = build(bundle_dir, previous)
    out_dir = Path(out) if out else Path(bundle_dir)
    (out_dir / "kpis.json").write_text(json.dumps(data, indent=1))
    template = (Path(__file__).resolve().parent / "templates" / "kpis.html").read_text()
    (out_dir / "kpis.html").write_text(template.replace("/*__KPIS__*/null", json.dumps(data).replace("</", "<\\/")))
    return out_dir / "kpis.html"
