#!/usr/bin/env python3
"""Run the BAML test corpus through a `baml pack --tests --rust` binary.

Lifted from brr/runtime-v2's rt_corpus.py, reduced to what the shared-runtime
pack needs: one test binary, run with the interpreter (BAML_COMPILED_BODIES=0),
with compiled bodies, and with compiled bodies under BAML_GC_STRESS=1. Each
test's verdict under compiled code must equal its verdict under the
interpreter; the sorted failure messages must agree too.

Per test: match, mismatch, crash (the compiled run did not finish with a
report), or flaky: a mismatch whose verdict varies when the test is rerun
alone in one mode, the interpreter's included, so it races by itself.
Function admission (compiled vs fallback) comes from
BAML_PACK_ADMISSION_REPORT. Exits non-zero on any mismatch or crash.

    uv run --no-project rt_corpus.py --baml-cli <baml-cli> --workspace <baml_language>
"""
from __future__ import annotations

import argparse
from collections import Counter
import json
import os
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
VERDICTS = ("PASS", "FAIL", "TOLERATED")
MODES = {
    "interpreter": {"BAML_COMPILED_BODIES": "0"},
    "compiled": {"BAML_COMPILED_BODIES": "1"},
    "gc-stress": {"BAML_COMPILED_BODIES": "1", "BAML_GC_STRESS": "1"},
}


def pack(args, out: Path) -> tuple[Path, Path, float]:
    """Pack the corpus tests; the binary, the admission report, wall seconds."""
    binary = out / "corpus-tests"
    admission = out / "admission.tsv"
    env = dict(os.environ, BAML_CLI_ALLOW_DIRECT="1", BAML_HOME=str(out / "home"),
               BAML_PACK_ADMISSION_REPORT=str(admission))
    (out / "home").mkdir(parents=True, exist_ok=True)
    argv = [args.baml_cli, "--agent-skill-check", "off", "pack", "--tests", "--rust",
            "--rust-profile", args.profile, "--rust-workspace", args.workspace,
            "--rust-build-dir", str(out / "build"), "--from", args.source, "-o", str(binary)]
    started = time.monotonic()
    with (out / "pack.log").open("w") as log:
        code = subprocess.call(argv, env=env, stdout=log, stderr=subprocess.STDOUT)
    if code:
        sys.exit(f"pack failed ({code}); see {out / 'pack.log'}")
    return binary, admission, time.monotonic() - started


def namespace(name: str) -> str:
    """The top-level namespace of a canonical test ID: `root.ns_x`."""
    head = name.split("::", 1)[0]
    return ".".join(head.split(".")[:2])


def run_isolated(binary: Path, mode: str, out: Path, timeout: int, names) -> dict:
    """Run `mode` one namespace at a time, so a crash costs only its tests."""
    verdicts, messages, crashed = {}, [], []
    for ns in sorted({namespace(name) for name in names}):
        result = run(binary, mode, out, timeout, ["-i", f"{ns}::*", "-i", f"{ns}.*"], ns)
        if "crash" in result:
            crashed.append(ns)
            continue
        verdicts.update(result["verdicts"])
        messages.extend(result["messages"])
    return {"verdicts": verdicts, "messages": sorted(messages), "crashed": crashed}


def run(binary: Path, mode: str, out: Path, timeout: int, selectors=(), label=None) -> dict:
    """Run the test binary in `mode`; its verdicts and messages, or a crash."""
    env = {key: value for key, value in os.environ.items() if key != "BAML_GC_STRESS"}
    env.update(NO_COLOR="1", BAML_TELEMETRY="medium", BAML_HOME=str(out / "home"),
               **MODES[mode])
    started = time.monotonic()
    try:
        done = subprocess.run([str(binary), *selectors], env=env, capture_output=True, timeout=timeout,
                              text=True, errors="surrogateescape")
    except subprocess.TimeoutExpired:
        return {"crash": f"timed out after {timeout}s"}
    stem = mode if label is None else f"{mode}.{label}"
    (out / f"{stem}.stdout").write_text(done.stdout, errors="surrogateescape")
    (out / f"{stem}.stderr").write_text(done.stderr, errors="surrogateescape")
    if done.returncode not in (0, 2) or "AGGREGATE" not in done.stdout:
        return {"crash": f"exit {done.returncode}", "seconds": time.monotonic() - started}
    verdicts, messages = {}, []
    for line in done.stdout.splitlines():
        head, _, rest = line.partition(" ")
        if head in VERDICTS:
            verdicts[rest] = head
        elif line.startswith("  => "):
            messages.append(line[5:])
    return {"verdicts": verdicts, "messages": sorted(messages),
            "seconds": time.monotonic() - started}


RERUNS = 5


def stable(binary: Path, mode: str, out: Path, timeout: int, name: str) -> set:
    """The verdicts `name` gets over RERUNS runs of `mode`, alone."""
    seen = set()
    for i in range(RERUNS):
        result = run(binary, mode, out, timeout, [name], f"rerun{i}")
        seen.add(result.get("verdicts", {}).get(name, "crash"))
    return seen


def separate_flaky(binary, out, timeout, reference, runs, categories) -> dict:
    """Rerun each mismatch alone: a test whose verdict varies between runs of
    one mode (the interpreter's included) races by itself and is flaky, not a
    compiled-code divergence. Returns test -> evidence."""
    flaky = {}
    for name in [n for n, c in categories.items() if c == "mismatch"]:
        evidence = {"interpreter": sorted(stable(binary, "interpreter", out, timeout, name))}
        for mode in runs:
            evidence[mode] = sorted(stable(binary, mode, out, timeout, name))
        if any(len(verdicts) > 1 for verdicts in evidence.values()):
            flaky[name] = evidence
            categories[name] = "flaky"
    return flaky


def admission_counts(admission: Path) -> Counter:
    counts = Counter()
    if admission.exists():
        for line in admission.read_text().splitlines():
            counts[line.split("\t", 1)[0]] += 1
    return counts


def classify(reference: dict, runs: dict) -> tuple[dict, list]:
    """Per-test category across every compiled mode, and message mismatches."""
    categories = {}
    problems = []
    for name, verdict in sorted(reference["verdicts"].items()):
        category = "match"
        for mode, result in runs.items():
            if "crash" in result or namespace(name) in result.get("crashed", ()):
                category = "crash"
                break
            if result["verdicts"].get(name) != verdict:
                category = "mismatch"
                problems.append(f"{mode}: {name}: interpreter {verdict}, "
                                f"compiled {result['verdicts'].get(name)}")
        categories[name] = category
    for mode, result in runs.items():
        if "crash" in result:
            problems.append(f"{mode}: {result['crash']}")
        elif result.get("crashed"):
            problems.append(f"{mode}: crashed in {', '.join(result['crashed'])}")
        elif result["messages"] != reference["messages"]:
            problems.append(f"{mode}: failure messages differ from the interpreter's")
    return categories, problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--baml-cli", required=True)
    parser.add_argument("--workspace", required=True, help="the baml_language checkout")
    parser.add_argument("--source", default=str(HERE.parent / "baml_src"))
    parser.add_argument("--out", default=None, help="evidence directory")
    parser.add_argument("--profile", default="fast", choices=("fast", "release"))
    parser.add_argument("--binary", default=None, help="reuse a packed binary")
    parser.add_argument("--timeout", type=int, default=3600)
    args = parser.parse_args()
    out = Path(args.out or Path(args.workspace).parent / "target" / "rt-corpus").resolve()
    out.mkdir(parents=True, exist_ok=True)
    if args.binary:
        binary, admission, pack_seconds = Path(args.binary), out / "admission.tsv", None
    else:
        binary, admission, pack_seconds = pack(args, out)
    reference = run(binary, "interpreter", out, args.timeout)
    if "crash" in reference:
        sys.exit(f"the interpreter run did not finish: {reference['crash']}")
    runs = {}
    for mode in ("compiled", "gc-stress"):
        runs[mode] = run(binary, mode, out, args.timeout)
        if "crash" in runs[mode]:
            runs[mode] = run_isolated(binary, mode, out, args.timeout, reference["verdicts"])
    categories, problems = classify(reference, runs)
    flaky = separate_flaky(binary, out, args.timeout, reference, runs, categories)
    problems = [p for p in problems if not any(f": {name}: " in p for name in flaky)]
    totals = Counter(categories.values())
    functions = admission_counts(admission)
    summary = {
        "tests": len(categories),
        "match": totals["match"],
        "mismatch": totals["mismatch"],
        "unsupported-fallback": totals["unsupported-fallback"],
        "crash": totals["crash"],
        "flaky (verdict varies within one mode)": flaky,
        "functions compiled": functions["compiled"],
        "functions fallback": functions["fallback"],
        "pack seconds": pack_seconds,
        "run seconds": {mode: r.get("seconds") for mode, r in
                        {"interpreter": reference, **runs}.items()},
        "problems": problems,
    }
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    return 1 if totals["mismatch"] or totals["crash"] or problems else 0


if __name__ == "__main__":
    sys.exit(main())
