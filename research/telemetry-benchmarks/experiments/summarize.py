"""Collect every result file into site/data.json for the website.

    python experiments/summarize.py
"""
import json
import statistics
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RES = ROOT / "results"


def load(name):
    p = RES / name
    return json.loads(p.read_text()) if p.exists() else []


def wire():
    rows = load("wire-hosted.json") + [r for r in load("wire-python-local.json") if r["stack"] == "otel"]
    rows += load("wire-nobodies.json")
    # Earlier "10v" runs varied only a short suffix, which gzip compresses across calls.
    # wire-distinct.json reruns 10v with independent documents and replaces those rows.
    rows += [r for r in load("wire-distinct.json") if not r.get("error")]
    out = []
    for r in rows:
        t = r["telemetry"]
        billing = t.get("billing") or {}
        model_ok = sum(1 for s in r["model_http"]["statuses"] if s == 200)
        out.append({
            "stack": r["stack"], "app": r["app"], "size": r["size"], "batch": r["batch"], "calls": r["calls"],
            "bytes": t["telemetry_bytes"], "spans": t.get("spans"),
            "billed": billing.get("logical_bytes"), "model_calls": len(r["model_http"]["statuses"]),
            "model_ok": model_ok, "prompt_bytes": r["model_http"]["sent"],
        })
    # One row per (stack, app, size, batch); bridge-nobodies and bridge may appear twice.
    seen = {}
    for r in out:
        seen[(r["stack"], r["app"], r["size"], r["batch"])] = r
    return list(seen.values())


def overhead():
    by = defaultdict(list)
    for r in load("overhead.json"):
        by[r["stack"]].append(r)
    out = {}
    for stack, rs in by.items():
        warm = sorted(x for r in rs for x in r["warm_ms"])
        out[stack] = {
            "n": len(rs),
            "wall_ms": statistics.median(r["wall_ms"] for r in rs),
            "rss_mb": statistics.median(r["peak_rss_mb"] for r in rs),
            "first_ms": statistics.median(r["first_ms"] for r in rs),
            "warm_ms": statistics.median(warm),
            "warm_p95_ms": warm[int(0.95 * len(warm))],
            "telemetry_bytes": statistics.median(r["telemetry_bytes"] for r in rs),
        }
    return out


def crash():
    by = defaultdict(list)
    for r in load("crash.json"):
        by[(r["stack"], r["signal"])].append(r)
    out = []
    for (stack, sig), rs in sorted(by.items()):
        done = sum(r["model_calls_completed"] for r in rs)
        got = sum(min(r["model_spans_delivered"], r["model_calls_completed"]) for r in rs)
        out.append({"stack": stack, "signal": sig, "trials": len(rs), "completed": done, "delivered": got,
                    "lost_pct": 100 * (done - got) / done if done else None,
                    "trials_detail": [{"after_s": r["after_s"], "completed": r["model_calls_completed"],
                                       "delivered": r["model_spans_delivered"]} for r in rs]})
    return out


def searchable():
    by = defaultdict(list)
    for r in load("searchable.json"):
        if r["seconds"]:
            by[r["vendor"]].append(r["seconds"])
    return {v: {"median": statistics.median(s), "min": min(s), "max": max(s), "n": len(s)} for v, s in by.items()}


def anatomy():
    out = []
    for r in load("logfire-anatomy.json"):
        gaps = r["copy_gaps"]
        out.append({k: r[k] for k in ("content", "pattern", "size", "calls", "sent", "decoded", "lzma",
                                      "doc_gzip_alone", "spans", "pending_spans", "categories")}
                   | {"gap_median": gaps[len(gaps) // 2] if gaps else None,
                      "gap_min": gaps[0] if gaps else None, "gap_max": gaps[-1] if gaps else None})
    return out


def wire_english():
    return [{"stack": r["stack"], "size": r["size"], "calls": r["calls"], "bytes": r["telemetry"]["telemetry_bytes"],
             "billed": (r["telemetry"].get("billing") or {}).get("logical_bytes")}
            for r in load("wire-english.json") if not r.get("error")]


def main():
    data = {
        "wire": wire(), "overhead": overhead(), "crash": crash(), "searchable": searchable(),
        "loc": load("loc.json"), "scrub": load("scrub.json"), "pipeline": load("pipeline.json"),
        "anatomy": anatomy(), "wire_english": wire_english(),
    }
    out = ROOT / "site" / "data.json"
    out.parent.mkdir(exist_ok=True)
    out.write_text(json.dumps(data, indent=1) + "\n")
    html = (ROOT / "site" / "template.html").read_text().replace("__DATA__", json.dumps(data, separators=(",", ":")))
    (ROOT / "site" / "index.html").write_text(html)
    print("wrote", out, {k: len(v) for k, v in data.items()})


if __name__ == "__main__":
    main()
