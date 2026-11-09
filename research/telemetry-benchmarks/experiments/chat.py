"""E15: a multi-turn support chatbot. How do bytes and bills grow with conversation length?

Every model call resends the whole conversation (system prompt, user messages,
assistant replies, tool results). Each stack plays the same scripted
conversation (lab/chat_script.py) against the same mock model, for several
conversation lengths, one fresh process per run.

    python experiments/chat.py --out results/chat.json            # BAML uploads go to the real BCS
    python experiments/chat.py --local --out results/chat-local.json
"""
import argparse
import json
import random
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "lab"))
sys.path.insert(0, str(ROOT / "env" / "proto"))
sys.path.insert(0, str(ROOT / "experiments"))
from chat_script import (SYSTEM, SYSTEM_BIG, SYSTEM_FILES, file_users, make_reply_rule, photo_b64,  # noqa: E402
                         users, write_files)
from logical_usage import Meter  # noqa: E402
from servers import bcs_relay, mock_llm, relay  # noqa: E402
from wire import BAML, SECRETS, run_python, summarize_bcs, summarize_otlp  # noqa: E402


def run_baml(args_json, model, bcs, function="Converse"):
    env = {"PATH": "/usr/bin:/bin", "HOME": str(Path.home()), "MODEL_BASE_URL": model.url + "/v1",
           "MODEL_API_KEY": "fixture", "BAML_TELEMETRY": "medium",
           "BOUNDARY_API_KEY": bcs.handle, "BOUNDARY_API_URL": bcs.url}
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        f.write(args_json)
    cmd = [BAML, "--agent-skill-check", "off", "run", function, "--output-format", "json", "--log", "off",
           "--", "--json-args", "@" + f.name]
    proc = subprocess.run(cmd, env=env, cwd=ROOT / "apps" / "baml_chat", capture_output=True, text=True, timeout=600)
    Path(f.name).unlink()
    if proc.returncode:
        raise RuntimeError(proc.stderr[-1500:].replace(bcs.handle, "[key]"))
    return int(proc.stdout.strip())


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--stacks", nargs="+", default=["baml", "logfire", "datadog", "otel"])
    p.add_argument("--turns", nargs="+", type=int, default=[1, 2, 3, 5, 8, 10, 15, 20, 30, 40, 50])
    p.add_argument("--local", action="store_true", help="answer BCS uploads locally instead of forwarding")
    p.add_argument("--variant", default="base", choices=["base", "photo", "bigsystem", "files", "longchat"],
                   help="photo: a 150 KB image attached before turn 1; bigsystem: a 20 KB system prompt; "
                        "files: a coding agent that reads a 3-150 KB file every turn; "
                        "longchat: 1.5 KB user messages and 2.5 KB replies, no tools")
    p.add_argument("--model-delay", type=float, default=0.0,
                   help="seconds the mock model waits per call; a real model takes about a second")
    p.add_argument("--seed", type=int, default=int(time.time()))
    p.add_argument("--out", required=True)
    args = p.parse_args()
    out = ROOT / args.out
    rows = json.loads(out.read_text()) if out.exists() else []
    plan = [(s, t) for s in args.stacks for t in args.turns]
    random.Random(args.seed).shuffle(plan)
    print(f"{len(plan)} runs", flush=True)
    for i, (stack, turns) in enumerate(plan):
        # A fresh user script per run, so hosted BCS can't reuse an earlier run's uploads.
        run_seed = args.seed * 100 + i
        rule = make_reply_rule(2500 if args.variant == "longchat" else 600)
        files_dir = None
        if args.variant == "files":
            files_dir = tempfile.mkdtemp(prefix="chatfiles-")
            script = {"system": SYSTEM_FILES, "users": file_users(write_files(files_dir, turns, run_seed), run_seed),
                      "photo": None, "files": files_dir}
        elif args.variant == "longchat":
            script = {"system": SYSTEM, "users": users(turns, seed=run_seed, chars=1500, tools=False)}
        else:
            script = {"system": SYSTEM_BIG if args.variant == "bigsystem" else SYSTEM,
                      "users": users(turns, seed=run_seed)}
        if args.variant == "photo":
            script["photo"] = photo_b64(run_seed)
        model, tele, bcs, meter = mock_llm(rule=rule, delay_s=args.model_delay), None, None, None
        started = datetime.now(timezone.utc).isoformat()
        t_start = time.time()
        t_exit = None
        try:
            if stack == "baml":
                meter = Meter()
                bcs = bcs_relay(None if args.local else SECRETS["BOUNDARY_API_KEY"], meter=meter)
                if args.variant == "photo":
                    baml_args = {"system": script["system"], "photo_b64": script["photo"], "users": script["users"]}
                    calls = run_baml(json.dumps(baml_args), model, bcs, "ConversePhoto")
                elif args.variant == "files":
                    baml_args = {"system": script["system"], "dir": files_dir, "users": script["users"]}
                    calls = run_baml(json.dumps(baml_args), model, bcs, "ConverseFiles")
                else:
                    calls = run_baml(json.dumps(script), model, bcs)
                t_exit = time.time()
                time.sleep(0.2)
                telemetry = summarize_bcs(bcs.events, meter)
            else:
                tele = relay() if stack != "off" else None
                run_python(stack, "chat", json.dumps(script), 1, False, model, tele)
                t_exit = time.time()
                time.sleep(0.3)
                telemetry = summarize_otlp(tele.events) if tele else {"telemetry_bytes": 0, "spans": 0}
                telemetry.pop("fields", None)
                calls = None
        except Exception as exc:
            print(f"[{i+1}/{len(plan)}] {stack} {turns} FAILED: {exc}", flush=True)
            rows.append(dict(stack=stack, turns=turns, error=str(exc)[:2000], utc=started))
            continue
        finally:
            for server in (model, tele, bcs):
                if server:
                    server.shutdown()
            if files_dir:
                import shutil
                shutil.rmtree(files_dir, ignore_errors=True)
        reqs = model.requests
        events = (bcs.events if bcs else tele.events if tele else [])
        tele_times = [(e["t"], e["sent"]) for e in events if "t" in e]
        puts = [(e["t0"], e["dur"], e["sent"]) for e in events if e.get("operation") == "upload" and "dur" in e]
        last_call = max(r["t"] for r in reqs) if reqs else None
        timing = {"start": t_start, "exit": t_exit, "model_t": [r["t"] for r in reqs], "telemetry": tele_times, "puts": puts,
                  "wall_s": t_exit - t_start, "exit_after_last_call_s": t_exit - last_call if last_call else None,
                  "bytes_after_last_call": sum(b for t, b in tele_times if last_call and t > last_call)}
        row = dict(stack=stack, turns=turns, hosted=not args.local and stack == "baml", utc=started, model_delay=args.model_delay, variant=args.variant, telemetry=telemetry,
                   model_http={"requests": len(reqs), "sent": sum(r["sent"] for r in reqs),
                               "per_call_sent": [r["sent"] for r in reqs], "statuses": [r["status"] for r in reqs]},
                   baml_calls=calls, timing=timing)
        rows.append(row)
        out.write_text(json.dumps(rows, indent=1) + "\n")
        print(f"[{i+1}/{len(plan)}] {stack:8} turns={turns:>3} model_calls={len(reqs):>3} prompt={row['model_http']['sent']:>9}B "
              f"wire={telemetry.get('telemetry_bytes'):>9}B spans={telemetry.get('spans')} "
              f"billed={(telemetry.get('billing') or {}).get('logical_bytes')}", flush=True)


if __name__ == "__main__":
    main()
