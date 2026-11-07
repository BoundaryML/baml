"""E6: what reaches the backend when a worker process dies?

Each trial starts a process that makes `classify` calls in a loop against a mock
model that takes 50 ms per reply. After `--after` seconds the process is sent
SIGKILL (a crash, an OOM kill) or SIGTERM (a deploy, a container stop). Then we
compare model calls that completed with calls whose trace reached the local
stand-in backend.

    python experiments/crash.py --trials 5 --out results/crash.json
"""
import argparse
import json
import os
import random
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "lab"))
sys.path.insert(0, str(ROOT / "env" / "proto"))
from logical_usage import Meter  # noqa: E402
from servers import bcs_relay, mock_llm, relay  # noqa: E402

PY = str(ROOT / "env" / "py" / "bin" / "python")
BAML = str(ROOT / "bin" / "baml")


def model_spans(stack, sink, meter):
    """Count delivered traces of model calls (one per classify call)."""
    if stack in ("baml", "bridge"):
        return meter.names.get("network_announcement", 0)
    llm = 0
    for e in sink.events:
        if e.get("fields"):
            # Completed model spans carry the response id; Logfire's pending spans do not.
            llm += sum(1 for _, key, _ in e["fields"] if key == "gen_ai.response.id")
        elif e.get("json_events"):
            llm += e["json_events"] // 2  # Datadog LLM Obs: a workflow span plus an llm span per call
    return llm


def trial(stack, sig, after):
    model = mock_llm(delay_s=0.05)
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BOUNDARY_", "DD_", "LOGFIRE_", "OTEL_"))}
    env.update(MODEL_BASE_URL=model.url + "/v1", MODEL_API_KEY="fixture")
    meter = Meter()
    if stack in ("baml", "bridge"):
        sink = bcs_relay(None, meter=meter)
        env.update(BOUNDARY_API_KEY=sink.handle, BOUNDARY_API_URL=sink.url, BAML_TELEMETRY="medium")
    else:
        sink = relay()
        env.update(TELEMETRY_URL=sink.url, LOGFIRE_TOKEN="pylf_v1_us_" + "0" * 40, DD_API_KEY="0" * 32)
    if stack == "baml":
        cmd = [BAML, "--agent-skill-check", "off", "run", "Many", "--log", "off", "--",
               "--app", "classify", "--text", "crash test", "--n", "100000", "--vary", "true"]
        cwd = ROOT / "apps" / "baml"
    else:
        if stack == "bridge":
            env["PYTHONPATH"] = str(ROOT / "apps" / "py_bridge")
        cmd = [PY, str(ROOT / "apps" / "py_apps.py"), stack, "classify", "--text", "crash test", "--n", "100000", "--vary"]
        cwd = ROOT
    proc = subprocess.Popen(cmd, env=env, cwd=cwd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(after)
    completed_at_signal = len(model.requests)
    proc.send_signal(sig)
    try:
        proc.wait(timeout=30)
    except subprocess.TimeoutExpired:
        proc.kill()
    time.sleep(2)
    completed = len(model.requests)
    delivered = model_spans(stack, sink, meter)
    model.shutdown()
    sink.shutdown()
    return {"stack": stack, "signal": signal.Signals(sig).name, "after_s": after,
            "model_calls_completed": completed, "calls_at_signal": completed_at_signal,
            "model_spans_delivered": delivered, "requests_to_backend": len(sink.events),
            "lost": max(0, completed - delivered)}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--stacks", nargs="+", default=["baml", "bridge", "logfire", "otel", "datadog"])
    p.add_argument("--signals", nargs="+", default=["SIGKILL", "SIGTERM"])
    p.add_argument("--after", type=float, default=-1, help="seconds before the signal; -1 = random 3-9 s")
    p.add_argument("--trials", type=int, default=3)
    p.add_argument("--out", required=True)
    args = p.parse_args()
    plan = [(s, g) for _ in range(args.trials) for s in args.stacks for g in args.signals]
    random.Random(3).shuffle(plan)
    rows = []
    rng = random.Random(5)
    for stack, sig in plan:
        # A random kill time keeps the kill from lining up with any exporter's flush interval.
        after = args.after if args.after > 0 else round(rng.uniform(3, 9), 2)
        row = trial(stack, getattr(signal, sig), after)
        rows.append(row)
        print(row, flush=True)
        (ROOT / args.out).write_text(json.dumps(rows, indent=1) + "\n")


if __name__ == "__main__":
    main()
