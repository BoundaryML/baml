#!/usr/bin/env python3
"""Seed the experiments Convex project's model_benchmarking table from this
experiment's outputs (results.json + per-model bench reports).

    python3 seed_convex.py          # seeds prod (giddy-coyote-574)

Idempotent: the seed mutation replaces the table's contents. Never touches
the turns table.
"""

import json
import pathlib
import subprocess

HERE = pathlib.Path(__file__).resolve().parent
CONVEX_DIR = HERE.parent / "benchmarking-realtime" / "convex-experiments"

# Per-model context the raw numbers don't carry: compile status (from
# `baml check` on each project), rough builder spend, and the writeup that
# shows on the experiments site.
NOTES = {
    "openai-gpt-5-nano": dict(compileErrors=407, spendUsd=0.02, summary=(
        "Top score, and it doesn't compile. nano wrote 13 chunks of fluent-looking BAML, "
        "more code than anyone else, while running the fewest describe calls. It ignored "
        "all five failing checks. The grader found nothing to object to because the shapes "
        "match how the language wants to be used; the compiler rejects almost every line. "
        "Fluency without grounding."
    )),
    "anthropic-claude-sonnet-4-6": dict(compileErrors=6, spendUsd=0.85, summary=(
        "The careful one. 31 describe calls before it committed to anything, and at one "
        "point its project compiled clean. Then a late edit broke it and the turn budget "
        "ran out. Its two dings are real: an LLM function nothing tests, and a braceless "
        "lambda inside a reduce."
    )),
    "openai-gpt-5-mini": dict(compileErrors=57, spendUsd=0.04, summary=(
        "Zero slop findings, four omissions. mini writes tidy code and avoids the "
        "primitives it never learned. Dates kept as strings, hand-rolled if-chains where "
        "pattern matching fits, loops that mutate where iterators exist. Clean text with "
        "real gaps, and 57 compile errors underneath."
    )),
    "anthropic-claude-haiku-4-5": dict(compileErrors=0, spendUsd=0.28, summary=(
        "Fourth place, and the only project the compiler accepts. haiku kept a steady "
        "loop going (8 describes, 9 checks, 10 file writes) and shipped working code with "
        "honest slop in it: legacy test syntax, an untested LLM function, and a prompt "
        "that hand-describes the output JSON instead of letting the type do it. That last "
        "one graded a 1."
    )),
    "openai-gpt-5_1": dict(compileErrors=12, spendUsd=0.25, summary=(
        "Wrote TypeScript in .baml files. Arrow-function lambdas, a return on every final "
        "expression, legacy test blocks. It even reinvented the LLM-call primitive by hand "
        "instead of declaring an LLM function. Seven slop findings, 12 compile errors, and "
        "its five checks never converged. Lowest score, and it earned it."
    )),
}


def main() -> None:
    results = json.loads((HERE / "results.json").read_text())
    rows = []
    for r in results:
        name = r["model"]  # e.g. "openai-gpt-5-nano" (the run-dir name)
        provider, model = name.split("-", 1)
        rpt = json.loads((HERE / "runs" / name / "bench" / "report.json").read_text())
        slop = [
            {"chunkId": f["chunk_id"], "cardId": f["card_id"], "grade": f["grade"],
             "evidence": f["evidence"][:300]}
            for f in rpt["findings"]
            if f["verdict"] in ("fighting", "reinventing") and not f["refuted"]
        ]
        omissions = [
            {"cardId": o["card_id"], "description": o["description"][:300]}
            for o in rpt["omissions"]
        ]
        notes = NOTES.get(name, dict(compileErrors=-1, spendUsd=None, summary=""))
        rows.append({
            "model": model.replace("_", "."),
            "provider": provider,
            "adherence": r["adherence"],
            "commission": r["commission"],
            "omission": r["omission"],
            "chunks": r["chunks"],
            "pairs": r["pairs"],
            "slop": r["slop"],
            "compileErrors": notes["compileErrors"],
            "spendUsd": notes["spendUsd"],
            "summary": notes["summary"],
            "slopFindings": slop,
            "omissions": omissions,
        })
    payload = json.dumps({"rows": rows})
    out = subprocess.run(
        ["npx", "convex", "run", "modelBenchmarking:seed", payload, "--prod"],
        cwd=CONVEX_DIR, capture_output=True, text=True,
    )
    print(out.stdout.strip() or out.stderr.strip())
    print(f"seeded {len(rows)} rows")


if __name__ == "__main__":
    main()
