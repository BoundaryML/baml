"""Collect every result file into site/data.json for the website.

    python experiments/summarize.py
"""
import json
import sys
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


def chat():
    """Chatbot runs with a 1 s model delay (BAML uploads to the real BCS), plus the earlier instant-model runs."""
    rows = []
    sources = [(f"chat-{st}.json", "") for st in ("baml", "logfire", "datadog", "otel")]
    sources += [("chat-instant.json", "-instant"), ("chat-instant-hosted.json", "-instant-hosted")]
    for name, suffix in sources:
        for r in load(name):
            if r.get("error"):
                continue
            t = r["telemetry"]
            b = t.get("billing") or {}
            rows.append({"stack": r["stack"] + suffix, "turns": r["turns"],
                         "model_calls": r["model_http"]["requests"], "prompt_bytes": r["model_http"]["sent"],
                         "per_call_sent": r["model_http"]["per_call_sent"], "bytes": t["telemetry_bytes"],
                         "upload": (t.get("by_operation") or {}).get("upload"),
                         "already_available": t.get("already_available"), "puts": t.get("puts"),
                         "planned_uploads": t.get("planned_uploads"),
                         "spans": t.get("spans"), "billed": b.get("logical_bytes")})
    return rows


def chat_variants():
    """Chat with a photo or a 20 KB system prompt (1 s model; BAML on the real BCS), plus the base runs."""
    rows = []
    for v in ("photo", "bigsystem"):
        for st in ("baml", "logfire", "datadog", "otel", "baml-nocache"):
            for r in load(f"chat-{v}-{st}.json"):
                if r.get("error"):
                    continue
                t = r["telemetry"]
                rows.append({"variant": v, "stack": st, "turns": r["turns"], "model_calls": r["model_http"]["requests"],
                             "prompt_bytes": r["model_http"]["sent"], "last_call": r["model_http"]["per_call_sent"][-1],
                             "bytes": t["telemetry_bytes"],
                             "upload": (t.get("by_operation") or {}).get("upload"),
                             "billed": (t.get("billing") or {}).get("logical_bytes")})
    for r in chat():
        if r["stack"] in ("baml", "logfire", "datadog", "otel") and r["turns"] in (1, 5, 10, 20):
            rows.append({"variant": "base", **{k: r[k] for k in ("stack", "turns", "model_calls", "prompt_bytes", "bytes", "upload", "billed")}})
    return {"rows": rows, "crossprocess": load("chat-photo-crossprocess.json")}


def chat_big():
    """Bigger conversations: a coding agent that reads files (to ~500k tokens) and a long chat of
    small messages (to ~100k tokens). Totals per stack, plus BCS's per-call timeline for the longest run."""
    rows, timelines = [], {}
    for v in ("files", "longchat"):
        for r in load(f"chat-{v}.json"):
            if r.get("error"):
                continue
            t, m = r["telemetry"], r["model_http"]
            billing = t.get("billing") or {}
            rows.append({"variant": v, "stack": r["stack"], "turns": r["turns"], "model_calls": m["requests"],
                         "prompt_bytes": m["sent"], "last_call": m["per_call_sent"][-1], "bytes": t["telemetry_bytes"],
                         "upload": (t.get("by_operation") or {}).get("upload"), "billed": billing.get("logical_bytes"),
                         "hosted": r.get("hosted"), "already_available": t.get("already_available")})
            if r["stack"] != "baml" or not billing.get("timeline"):
                continue
            if v in timelines and timelines[v]["turns"] >= r["turns"]:
                continue
            # Cumulative upload bodies and billed bytes, closed at each model call (network announcement).
            points, up, bill, calls = [], 0, 0, 0
            for e in billing["timeline"]:
                up += e["body"]
                bill += e["billed"]
                if e["network_announcements"]:
                    calls += e["network_announcements"]
                    points.append([calls, up, bill])
            if points:
                points[-1][1:] = [up, bill]  # uploads after the last call (the closing batch) count toward it
            per_call = []
            prev_up = prev_bill = 0
            for (k, u, b) in points:
                per_call.append({"call": k, "uploaded": u - prev_up, "billed": b - prev_bill})
                prev_up, prev_bill = u, b
            timelines[v] = {"turns": r["turns"], "prompt_per_call": m["per_call_sent"], "per_call": per_call}
    return {"rows": rows, "timelines": timelines}


def lever_known():
    """Bytes of each conversation that are content BCS could reference instead of storing again:
    the system prompt (same in every conversation), the photo, and the files the agent read."""
    sys.path.insert(0, str(ROOT / "lab"))
    from chat_script import FILE_KB, SYSTEM, photo_b64
    files_turns = max((r["turns"] for r in load("chat-files.json") if r.get("stack") == "baml" and not r.get("error")), default=0)
    return {"chat": len(SYSTEM), "longchat": len(SYSTEM), "photo": len(SYSTEM) + len(photo_b64(1)),
            "files": sum(FILE_KB[t % len(FILE_KB)] * 1000 for t in range(files_turns))}


def flush():
    """How fast each stack gets a call's trace out, from the timed 48-turn agent run."""
    out = []
    for r in load("chat-files-timed.json"):
        tm = r.get("timing")
        if r.get("error") or not tm:
            continue
        mt = tm["model_t"]
        row = {"stack": r["stack"], "hosted": bool(r.get("hosted")), "calls": len(mt),
               "bytes": r["telemetry"]["telemetry_bytes"], "after_last": tm["bytes_after_last_call"],
               "exit_after_last_s": tm["exit_after_last_call_s"], "wall_s": tm["wall_s"],
               "last_call_s": mt[-1] - tm["start"]}
        if r["stack"] == "baml":
            billing = r["telemetry"]["billing"]
            lags, k = [], 0
            for e, put in zip(billing["timeline"], tm["puts"]):
                for _ in range(e["network_announcements"]):
                    if k < len(mt):
                        lags.append(put[0] + put[1] - mt[k])
                        k += 1
            big = [x for x in tm["puts"] if x[2] > 500_000]
            row.update(lag_median_s=statistics.median(lags), lag_max_s=max(lags),
                       upload_mbit_s=sum(x[2] for x in big) * 8 / 1e6 / sum(x[1] for x in big),
                       missing=billing["missing_sizes"])
        out.append(row)
    return out


def main():
    data = {
        "wire": wire(), "overhead": overhead(), "crash": crash(), "searchable": searchable(),
        "loc": load("loc.json"), "scrub": load("scrub.json"), "pipeline": load("pipeline.json"),
        "anatomy": anatomy(), "wire_english": wire_english(), "chat": chat(), "chat_variants": chat_variants(),
        "chat_big": chat_big(), "lever_known": lever_known(), "flush": flush(),
    }
    out = ROOT / "site" / "data.json"
    out.parent.mkdir(exist_ok=True)
    out.write_text(json.dumps(data, indent=1) + "\n")
    html = (ROOT / "site" / "template.html").read_text().replace("__DATA__", json.dumps(data, separators=(",", ":")))
    (ROOT / "site" / "index.html").write_text(html)
    print("wrote", out, {k: len(v) for k, v in data.items()})


if __name__ == "__main__":
    main()
