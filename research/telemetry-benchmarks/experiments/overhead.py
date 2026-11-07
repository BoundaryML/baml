"""E4/E5: what tracing costs in startup time, memory and per-call latency.

Every stack runs the same `classify` app (1 KiB input) against the loopback
mock model, exporting to a loopback stand-in for its vendor that answers
immediately, so no internet latency is included. Each process makes 1 + 50
calls: the first call is reported alone, the other 50 are "warm". Stacks run
in a shuffled order, `--reps` fresh processes each.

`--delay-ms` makes the mock model wait before replying, to show tracing as a
share of a realistic model call.

    python experiments/overhead.py --reps 12 --out results/overhead.json
"""
import argparse
import json
import os
import random
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "lab"))
sys.path.insert(0, str(ROOT / "env" / "proto"))
from servers import bcs_relay, mock_llm, relay  # noqa: E402

PY = str(ROOT / "env" / "py" / "bin" / "python")
BAML = str(ROOT / "bin" / "baml")
TEXT = "".join(random.Random(7).choice("abcdefghijklmnopqrstuvwxyz ") for _ in range(1024))
CALLS = 51


def clean_env():
    return {k: v for k, v in os.environ.items() if not k.startswith(("BOUNDARY_", "DD_", "LOGFIRE_", "OTEL_"))}


def run(stack, model, delay_ms):
    env = {**clean_env(), "MODEL_BASE_URL": model.url + "/v1", "MODEL_API_KEY": "fixture"}
    sink = None
    if stack in ("baml", "bridge", "baml-off", "bridge-off"):
        sink = bcs_relay(None)
        env.update(BOUNDARY_API_KEY=sink.handle, BOUNDARY_API_URL=sink.url,
                   BAML_TELEMETRY="off" if stack.endswith("-off") else "medium")
    elif stack != "off":
        sink = relay()
        env["TELEMETRY_URL"] = sink.url
        if stack == "logfire":
            env["LOGFIRE_TOKEN"] = "pylf_v1_us_" + "0" * 40
        if stack == "datadog":
            env["DD_API_KEY"] = "0" * 32
    if stack.startswith("baml"):
        # The BAML CLI times each call itself (see bench.baml) and prints the list.
        cmd = ["/usr/bin/time", "-l", BAML, "--agent-skill-check", "off", "run", "Timed", "--output-format", "json",
               "--log", "off", "--", "--text", TEXT, "--n", str(CALLS)]
        cwd = ROOT / "apps" / "baml_timed"
    else:
        py_stack = "bridge" if stack.startswith("bridge") else stack
        if py_stack == "bridge":
            env["PYTHONPATH"] = str(ROOT / "apps" / "py_bridge")
        cmd = ["/usr/bin/time", "-l", PY, str(ROOT / "apps" / "py_apps.py"), py_stack, "classify",
               "--text", TEXT, "--n", str(CALLS)]
        cwd = ROOT
    start = time.perf_counter()
    proc = subprocess.run(cmd, env=env, cwd=cwd, capture_output=True, text=True, timeout=600)
    wall = (time.perf_counter() - start) * 1000
    if sink:
        sink.shutdown()
    if proc.returncode:
        raise RuntimeError(proc.stderr[-1500:])
    rss = next(int(l.split()[0]) for l in proc.stderr.splitlines() if "maximum resident set size" in l)
    out = json.loads(proc.stdout.strip().splitlines()[-1])
    calls = out["call_ms"] if isinstance(out, dict) else [ns / 1e6 for ns in out]
    return {"wall_ms": wall, "peak_rss_mb": rss / 2**20, "first_ms": calls[0],
            "warm_median_ms": statistics.median(calls[1:]), "warm_ms": calls[1:],
            "setup_ms": out.get("setup_ms") if isinstance(out, dict) else None,
            "flush_ms": out.get("flush_ms") if isinstance(out, dict) else None,
            "telemetry_bytes": sum(e["sent"] for e in sink.events) if sink else 0}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--stacks", nargs="+", default=["off", "otel", "logfire", "datadog", "bridge-off", "bridge",
                                                   "baml-off", "baml"])
    p.add_argument("--reps", type=int, default=12)
    p.add_argument("--delay-ms", type=float, default=0)
    p.add_argument("--out", required=True)
    args = p.parse_args()
    plan = [s for _ in range(args.reps) for s in args.stacks]
    random.Random(11).shuffle(plan)
    rows = []
    for i, stack in enumerate(plan):
        model = mock_llm(delay_s=args.delay_ms / 1000)
        try:
            row = run(stack, model, args.delay_ms)
        finally:
            model.shutdown()
        row.update(stack=stack, delay_ms=args.delay_ms)
        rows.append(row)
        print(f"[{i+1}/{len(plan)}] {stack:10} wall={row['wall_ms']:.0f}ms rss={row['peak_rss_mb']:.0f}MB "
              f"first={row['first_ms']:.1f}ms warm={row['warm_median_ms']:.3f}ms", flush=True)
        (ROOT / args.out).write_text(json.dumps(rows, indent=1) + "\n")


if __name__ == "__main__":
    main()
