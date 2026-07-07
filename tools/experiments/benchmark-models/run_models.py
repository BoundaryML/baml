#!/usr/bin/env python3
# /// script
# dependencies = ["openai"]
# ///
"""Adherence-by-model experiment.

For each (provider, model): the model builds a small BAML project in a tool
loop (it can run `baml describe` / `baml check` / `baml fmt` and write files),
then baml-bench grades the result — same grader for everyone — and we compare
adherence scores.

    uv run run_models.py            # run everything not yet run
    uv run run_models.py --force    # rebuild everything
    uv run run_models.py --report   # just reprint the comparison table

Needs: OPENAI_API_KEY / ANTHROPIC_API_KEY (builders, metered), and
LLM_BASE_URL + LLM_API_KEY for the grader (the local claude-proxy works:
http://localhost:19090 / devproxytoken).

HARD COST LIMIT: builder API spend is metered from real token usage after
every call; crossing COST_LIMIT_USD aborts the whole sweep immediately.
"""

import argparse
import json
import os
import pathlib
import subprocess
import sys

from openai import OpenAI

HERE = pathlib.Path(__file__).resolve().parent
BENCH = HERE.parent.parent / "baml-bench" / "baml-bench"  # the packed grader CLI
RUNS = HERE / "runs"

COST_LIMIT_USD = 10.00

# ---- edit me: which models compete -------------------------------------------
# (provider, model, openai-compatible base_url, api-key env var)
MODELS = [
    ("openai", "gpt-5.1", "https://api.openai.com/v1", "OPENAI_API_KEY"),
    ("openai", "gpt-5-mini", "https://api.openai.com/v1", "OPENAI_API_KEY"),
    ("openai", "gpt-5-nano", "https://api.openai.com/v1", "OPENAI_API_KEY"),
    ("anthropic", "claude-sonnet-4-6", "https://api.anthropic.com/v1/", "ANTHROPIC_API_KEY"),
    ("anthropic", "claude-haiku-4-5", "https://api.anthropic.com/v1/", "ANTHROPIC_API_KEY"),
]

# $ per 1M tokens (input, output). Unknown models get a deliberately punitive
# fallback so the kill-switch errs toward stopping early.
PRICES = {
    "gpt-5.1": (1.25, 10.00),
    "gpt-5-mini": (0.25, 2.00),
    "gpt-5-nano": (0.05, 0.40),
    "claude-sonnet-4-6": (3.00, 15.00),
    "claude-haiku-4-5": (1.00, 5.00),
    "claude-opus-4-8": (15.00, 75.00),
}
FALLBACK_PRICE = (15.00, 75.00)

MAX_TURNS = 25


class CostLimitExceeded(RuntimeError):
    pass


class Meter:
    """Accumulates real spend from API usage; hard-stops the sweep at the limit."""

    def __init__(self, limit: float) -> None:
        self.limit = limit
        self.spent = 0.0

    def add(self, model: str, usage) -> None:
        inp, out = PRICES.get(model, FALLBACK_PRICE)
        cost = (usage.prompt_tokens * inp + usage.completion_tokens * out) / 1_000_000
        self.spent += cost
        if self.spent > self.limit:
            raise CostLimitExceeded(
                f"HARD STOP: builder spend ${self.spent:.2f} exceeded the ${self.limit:.2f} limit"
            )


METER = Meter(COST_LIMIT_USD)

TASK = """You are working in an empty directory that will become a small BAML project.
BAML is a typed, expression-oriented language for LLM functions. Your job:

Build a simple BAML project that extracts structured data from receipt text:
  - a `baml.toml` already exists; put source files in a `baml_src/` directory
  - a class or two modeling a receipt (vendor, total, line items...)
  - one LLM function that extracts a receipt from raw text
  - one or two pure helper functions (e.g. compute the total from line items)
  - a test or two for the pure helpers

You have NEVER seen BAML before, so learn it with the CLI before writing code:
`baml describe String`, `baml describe Array`, `baml describe Map` show stdlib
methods; `baml describe <YourSymbol>` works once code compiles; `baml check`
compiles the project; `baml fmt <file>` formats. Iterate until `baml check`
passes cleanly. Keep it simple and idiomatic. When you are done and the
project compiles, reply with just: DONE"""

TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "baml",
            "description": "Run a baml CLI command in the project directory. Pass only the arguments after `baml`, e.g. 'describe String' or 'check'.",
            "parameters": {
                "type": "object",
                "properties": {"args": {"type": "string"}},
                "required": ["args"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Write a file (relative path) in the project directory, creating parents.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "content": {"type": "string"},
                },
                "required": ["path", "content"],
            },
        },
    },
]


def run_tool(project: pathlib.Path, name: str, args: dict) -> str:
    if name == "write_file":
        rel = args["path"].lstrip("/")
        if ".." in rel.split("/"):
            return "error: path may not contain '..'"
        f = project / rel
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(args["content"])
        return f"wrote {rel} ({len(args['content'])} bytes)"
    if name == "baml":
        cmd = ["baml"] + args["args"].split()
        try:
            r = subprocess.run(cmd, cwd=project, capture_output=True, text=True, timeout=120)
        except subprocess.TimeoutExpired:
            return "error: command timed out"
        out = (r.stdout + r.stderr)[-6000:]
        return f"exit {r.returncode}\n{out}"
    return f"error: unknown tool {name}"


def build_project(model: str, base_url: str, key_env: str, project: pathlib.Path) -> None:
    client = OpenAI(base_url=base_url, api_key=os.environ[key_env], timeout=300)
    project.mkdir(parents=True, exist_ok=True)
    # Anchor project resolution: without a baml.toml the CLI walks UP the tree
    # and resolves the surrounding repo as the project (a model once spent all
    # its turns exploring baml-bench's own source).
    toml = project / "baml.toml"
    if not toml.exists():
        toml.write_text('[package]\nname = "receipts"\n')
    messages = [{"role": "user", "content": TASK}]
    for turn in range(MAX_TURNS):
        resp = client.chat.completions.create(model=model, messages=messages, tools=TOOLS)
        if resp.usage:
            METER.add(model, resp.usage)
        msg = resp.choices[0].message
        messages.append(msg.model_dump(exclude_none=True))
        if not msg.tool_calls:
            print(f"    [{model}] finished after {turn + 1} turns (spend ${METER.spent:.2f}): {(msg.content or '')[:80]}", flush=True)
            return
        for tc in msg.tool_calls:
            args = json.loads(tc.function.arguments)
            result = run_tool(project, tc.function.name, args)
            preview = args.get("args") or args.get("path")
            print(f"    [{model}] {tc.function.name}({preview}) -> {result.splitlines()[0][:60]}", flush=True)
            messages.append({"role": "tool", "tool_call_id": tc.id, "content": result})
    print(f"    [{model}] hit MAX_TURNS ({MAX_TURNS})", flush=True)


def grade(project: pathlib.Path, out: pathlib.Path) -> dict:
    r = subprocess.run(
        [str(BENCH), "--target", str(project), "--out", str(out)],
        capture_output=True, text=True, env=os.environ,
    )
    if not (out / "report.json").exists():
        raise RuntimeError(f"baml-bench produced no report: {r.stderr[-500:]}")
    return json.loads((out / "report.json").read_text())


def summarize() -> None:
    rows = []
    for d in sorted(RUNS.iterdir()) if RUNS.exists() else []:
        rpt = d / "bench" / "report.json"
        if rpt.exists():
            r = json.loads(rpt.read_text())
            slop = sum(1 for f in r["findings"] if f["verdict"] in ("fighting", "reinventing") and not f["refuted"])
            rows.append((d.name, r["adherence_score"], r["commission_score"], r["omission_score"], r["chunk_count"], r["graded_pairs"], slop))
    if not rows:
        print("no results yet")
        return
    rows.sort(key=lambda r: -r[1])
    hdr = f"{'model':34} {'adherence':>9} {'commission':>10} {'omission':>8} {'chunks':>6} {'pairs':>5} {'slop':>4}"
    print("\n" + hdr + "\n" + "-" * len(hdr))
    for name, a, c, o, ch, p, s in rows:
        print(f"{name:34} {a:9.2f} {c:10.2f} {o:8.2f} {ch:6} {p:5} {s:4}")
    (HERE / "results.json").write_text(json.dumps(
        [dict(zip(["model", "adherence", "commission", "omission", "chunks", "pairs", "slop"], r)) for r in rows], indent=2))
    print(f"\nwrote {HERE / 'results.json'}")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--force", action="store_true", help="rebuild projects that already exist")
    ap.add_argument("--report", action="store_true", help="only print the comparison table")
    args = ap.parse_args()
    if args.report:
        summarize()
        return
    if not os.environ.get("LLM_BASE_URL"):
        sys.exit("set LLM_BASE_URL/LLM_API_KEY for the grader (e.g. the local claude-proxy)")
    try:
        for provider, model, base_url, key_env in MODELS:
            if not os.environ.get(key_env):
                print(f"  skip {model}: {key_env} not set", flush=True)
                continue
            run_dir = RUNS / f"{provider}-{model}".replace("/", "-").replace(".", "_")
            project = run_dir / "project"
            if project.exists() and any(project.rglob("*.baml")) and not args.force:
                print(f"  {model}: project exists (use --force to rebuild)", flush=True)
            else:
                print(f"== building with {model} (spend so far ${METER.spent:.2f}) ==", flush=True)
                try:
                    build_project(model, base_url, key_env, project)
                except CostLimitExceeded:
                    raise
                except Exception as e:  # noqa: BLE001 — one model failing shouldn't kill the sweep
                    print(f"    [{model}] BUILD FAILED: {e}", flush=True)
                    continue
            print(f"== grading {model} ==", flush=True)
            try:
                r = grade(project, run_dir / "bench")
                print(f"    adherence {r['adherence_score']:.2f}  commission {r['commission_score']:.2f}  omission {r['omission_score']:.2f}", flush=True)
            except Exception as e:  # noqa: BLE001
                print(f"    [{model}] GRADE FAILED: {e}", flush=True)
    except CostLimitExceeded as e:
        print(f"\n!!! {e} — sweep aborted", flush=True)
    print(f"\ntotal builder spend: ${METER.spent:.2f} (limit ${COST_LIMIT_USD:.2f})")
    summarize()


if __name__ == "__main__":
    main()
