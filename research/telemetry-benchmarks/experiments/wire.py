"""E1/E2: bytes on the wire for the same four apps in every stack.

Each run is a fresh process. A loopback relay records every telemetry request
body. In `--hosted` mode the relay forwards the original bytes to the real
vendor (BCS, Logfire or Datadog) with the test-project key from ../.env.

    python experiments/wire.py --stacks off otel logfire datadog baml --out results/wire-local.json
    python experiments/wire.py --hosted --stacks baml logfire datadog --out results/wire-hosted.json
"""
import argparse
import json
import os
import random
import string
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "lab"))
sys.path.insert(0, str(ROOT / "env" / "proto"))
from servers import DATADOG_LLMOBS, LOGFIRE_INTAKE, bcs_relay, mock_llm, relay  # noqa: E402

SECRETS = dict(line.strip().split("=", 1) for line in open(ROOT / ".env") if "=" in line)
PY = str(ROOT / "env" / "py" / "bin" / "python")
BAML = str(ROOT / "bin" / "baml")

APPS = {"hello": "Hello", "classify": "Classify", "extract": "Extract", "agent": "Agent"}


def document(size, seed):
    """Low-repetition printable text of exactly `size` bytes, so compression can't hide size."""
    rng = random.Random(seed)
    alphabet = string.ascii_letters + string.digits + "     "
    return "".join(rng.choice(alphabet) for _ in range(size))


def agent_rule():
    """Fail the first reply attempt of every ticket with HTTP 503, then succeed."""
    from servers import default_rule
    state = {"replies": 0}

    def rule(prompt):
        if "Units available" in prompt:
            state["replies"] += 1
            if state["replies"] % 2 == 1:
                return "503"
        return default_rule(prompt)
    return rule


def run_python(stack, app, text, n, vary, model, tele):
    env = {**os.environ, **SECRETS, "MODEL_BASE_URL": model.url + "/v1", "MODEL_API_KEY": "fixture",
           "TELEMETRY_URL": tele.url if tele else ""}
    if stack == "logfire":
        env["LOGFIRE_TOKEN"] = SECRETS["LOGFIRE_TOKEN"]
    if stack == "bridge":
        for key in [k for k in env if k.startswith(("BOUNDARY_", "DD_", "LOGFIRE_"))]:
            del env[key]
        env.update(PYTHONPATH=str(ROOT / "apps" / "py_bridge"), BAML_TELEMETRY="medium",
                   BOUNDARY_API_KEY=tele.handle, BOUNDARY_API_URL=tele.url)
    cmd = [PY, str(ROOT / "apps" / "py_apps.py"), stack, app, "--text", text, "--n", str(n)] + (["--vary"] if vary else [])
    start = time.perf_counter()
    proc = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=300)
    wall = (time.perf_counter() - start) * 1000
    if proc.returncode:
        raise RuntimeError(proc.stderr[-1500:])
    return json.loads(proc.stdout.strip().splitlines()[-1]), wall


def run_baml(app, text, n, vary, model, bcs):
    env = {"PATH": os.environ["PATH"], "HOME": os.environ["HOME"], "MODEL_BASE_URL": model.url + "/v1",
           "MODEL_API_KEY": "fixture", "BAML_TELEMETRY": "medium"}
    if bcs is None:
        env["BOUNDARY_API_KEY"] = "local"
    else:
        env.update(BOUNDARY_API_KEY=bcs.handle, BOUNDARY_API_URL=bcs.url)
    args = ["--app", app, "--text", text, "--n", str(n), "--vary", "true" if vary else "false"]
    cmd = [BAML, "--agent-skill-check", "off", "run", "Many", "--output-format", "json", "--log", "off", "--", *args]
    start = time.perf_counter()
    proc = subprocess.run(cmd, env=env, cwd=ROOT / "apps" / "baml", capture_output=True, text=True, timeout=300)
    wall = (time.perf_counter() - start) * 1000
    if proc.returncode:
        raise RuntimeError(proc.stderr[-1500:].replace(bcs.handle if bcs else "~", "[key]"))
    return {"result": json.loads(proc.stdout)}, wall


def summarize_otlp(events):
    return {
        "requests": len(events),
        "telemetry_bytes": sum(e["sent"] for e in events),
        "decoded_bytes": sum(e.get("decoded", 0) for e in events),
        "spans": sum((e.get("spans") or e.get("json_events") or 0) for e in events),
        "statuses": sorted({e.get("upstream_status", 200) for e in events}),
        "rejections": [e["upstream_reply"] for e in events if e.get("upstream_reply")],
        "fields": [f for e in events for f in e.get("fields", [])],
    }


def summarize_bcs(events, meter):
    out = {
        "requests": len(events),
        "telemetry_bytes": sum(e["sent"] for e in events),
        "by_operation": {op: sum(e["sent"] for e in events if e["operation"] == op)
                         for op in ("heartbeat", "uploads:prepare", "upload")},
        "statuses": sorted({e["status"] for e in events}),
        "already_available": sum(e.get("already_available", 0) for e in events),
    }
    if meter:
        out["billing"] = meter.summary()
        out["spans"] = out["billing"]["spans"]
    return out


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--stacks", nargs="+", default=["off", "otel", "logfire", "datadog", "baml"])
    p.add_argument("--apps", nargs="+", default=list(APPS))
    p.add_argument("--sizes", nargs="+", type=int, default=[1024, 10240, 102400])
    p.add_argument("--batches", nargs="+", default=["1", "10v", "10s"],
                   help="1 = one call; 10v = ten varied calls; 10s = ten identical calls")
    p.add_argument("--hosted", action="store_true")
    p.add_argument("--seed", type=int, default=int(time.time()))
    p.add_argument("--out", required=True)
    args = p.parse_args()

    out_path = ROOT / args.out
    rows = json.loads(out_path.read_text()) if out_path.exists() else []
    plan = []
    for stack in args.stacks:
        for app in args.apps:
            for size in ([0] if app == "hello" else args.sizes):
                for batch in args.batches:
                    plan.append((stack, app, size, batch))
    random.Random(args.seed).shuffle(plan)
    print(f"{len(plan)} runs", flush=True)

    for i, (stack, app, size, batch) in enumerate(plan):
        n = 1 if batch == "1" else 10
        vary = batch.endswith("v")
        # A fresh seed per run keeps hosted content-addressed storage from reusing earlier uploads.
        text = document(size, seed=args.seed * 1000 + i) if size else ""
        model = mock_llm(rule=agent_rule()) if app == "agent" else mock_llm()
        meter = tele = bcs = None
        started = datetime.now(timezone.utc).isoformat()
        try:
            if stack == "bridge":
                from logical_usage import Meter
                meter = Meter()
                bcs = bcs_relay(SECRETS["BOUNDARY_API_KEY"] if args.hosted else None, meter=meter)
                timing, wall = run_python(stack, app, text, n, vary, model, bcs)
                time.sleep(0.2)
                telemetry = summarize_bcs(bcs.events, meter)
            elif stack == "baml":
                from logical_usage import Meter
                meter = Meter()
                bcs = bcs_relay(SECRETS["BOUNDARY_API_KEY"] if args.hosted else None, meter=meter)
                timing, wall = run_baml(app, text, n, vary, model, bcs)
                telemetry = summarize_bcs(bcs.events, meter)
            else:
                if args.hosted and stack == "logfire":
                    tele = relay(LOGFIRE_INTAKE)
                elif args.hosted and stack == "datadog":
                    tele = relay(DATADOG_LLMOBS)
                elif stack != "off":
                    tele = relay()
                timing, wall = run_python(stack, app, text, n, vary, model, tele)
                time.sleep(0.2)
                telemetry = summarize_otlp(tele.events) if tele else {"telemetry_bytes": 0, "spans": 0, "requests": 0}
        except Exception as exc:
            print(f"[{i+1}/{len(plan)}] {stack} {app} {size} {batch} FAILED: {exc}", flush=True)
            rows.append(dict(stack=stack, app=app, size=size, batch=batch, error=str(exc)[:2000], utc=started))
            continue
        finally:
            for server in (model, tele, bcs):
                if server:
                    server.shutdown()
        row = dict(stack=stack, app=app, size=size, batch=batch, calls=n, vary=vary, hosted=args.hosted,
                   utc=started, wall_ms=wall, timing=timing, telemetry=telemetry,
                   model_http={"requests": len(model.requests), "sent": sum(r["sent"] for r in model.requests),
                               "received": sum(r["received"] for r in model.requests),
                               "statuses": [r["status"] for r in model.requests]})
        rows.append(row)
        out_path.write_text(json.dumps(rows, indent=1) + "\n")
        print(f"[{i+1}/{len(plan)}] {stack:8} {app:8} {size:>6} {batch:>3}  tele={telemetry.get('telemetry_bytes')}B "
              f"spans={telemetry.get('spans')} billed={telemetry.get('billing', {}).get('logical_bytes')} "
              f"model={row['model_http']['sent']}B", flush=True)


if __name__ == "__main__":
    main()
