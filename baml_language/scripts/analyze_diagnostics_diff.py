#!/usr/bin/env python3
"""Analyze the V1 vs V2 diagnostics diff report.

Usage:
    # Deterministic analysis (no dependencies beyond python3):
    python3 scripts/analyze_diagnostics_diff.py

    # AI-assisted deep analysis (requires ANTHROPIC_API_KEY):
    python3 scripts/analyze_diagnostics_diff.py --with-ai

    # Custom paths:
    python3 scripts/analyze_diagnostics_diff.py --input path/to/diff.json --output path/to/report.md

Reads the JSON report produced by the v1_diagnostics_diff Rust test and produces
a structured markdown report for team review.
"""

import argparse
import json
import os
import re
import sys
from collections import defaultdict
from pathlib import Path

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------

SCRIPT_DIR = Path(__file__).resolve().parent
BAML_LANG_DIR = SCRIPT_DIR.parent
DEFAULT_INPUT = BAML_LANG_DIR / "crates" / "baml_tests" / "v1_v2_diagnostics_diff.json"
DEFAULT_OUTPUT = BAML_LANG_DIR / "crates" / "baml_tests" / "diagnostics_diff_report.md"
V1_ROOT = BAML_LANG_DIR.parent / "engine" / "baml-lib" / "baml" / "tests" / "validation_files"

LSP_INPUT = BAML_LANG_DIR / "crates" / "baml_tests" / "lsp_diagnostics_diff.json"
LSP_OUTPUT = BAML_LANG_DIR / "crates" / "baml_tests" / "lsp_diagnostics_diff_report.md"

# ---------------------------------------------------------------------------
# Error categorization heuristics
# ---------------------------------------------------------------------------

CATEGORY_PATTERNS = [
    ("cycle_detection", re.compile(r"dependency cycle|form a dependency cycle", re.I)),
    ("duplicate_name", re.compile(r"cannot be re-defined|already defined|is already defined", re.I)),
    ("duplicate_field", re.compile(r"duplicate field|already defined.*field", re.I)),
    ("duplicate_attribute", re.compile(r"can only be defined once|duplicate.*attribute", re.I)),
    ("unknown_type", re.compile(r"type.*does not exist|unknown type|unknown identifier", re.I)),
    ("unknown_field", re.compile(r"has no field|property.*does not exist|not exist on \w+", re.I)),
    ("unknown_variable", re.compile(r"variable.*does not exist|is undefined|unknown variable|unknown function", re.I)),
    ("type_mismatch", re.compile(r"cannot assign|expected type|expected.*but (found|got|received)", re.I)),
    ("reserved_keyword", re.compile(r"reserved keyword|reserved word", re.I)),
    ("config_validation", re.compile(r"property not known|missing.*property|unknown field|unrecognized field|must be (a |one of|non-negative)|must not be empty|http must be", re.I)),
    ("client_validation", re.compile(r"missing.*provider|client_response_type|remap_role|allowed_roles|fallback.*cycle", re.I)),
    ("constraint_validation", re.compile(r"error parsing jinja|@check|@assert|checks are not allowed", re.I)),
    ("jinja_warning", re.compile(r"instead of.*comparing enums|comparing enums with strings|is undefined, expected|referenced without parentheses|expects \d+ arguments|expects argument|did you mean.*alias|use one of:", re.I)),
    ("jinja_type_alias_warning", re.compile(r"is a type alias|is a recursive type alias", re.I)),
    ("test_validation", re.compile(r"test.*already defined|missing required arguments|missing.*args", re.I)),
    ("syntax_error", re.compile(r"does not start with any known|not an enum value|statement must end|line is invalid|attribute not known", re.I)),
    ("map_key_validation", re.compile(r"maps may only have|map keys must be", re.I)),
    ("attribute_validation", re.compile(r"type aliases may only have|@skip attribute|not allowed on test", re.I)),
    ("generator_validation", re.compile(r"property not known.*version|property not known.*output", re.I)),
    ("field_access_validation", re.compile(r"can only access fields|array index must be", re.I)),
]


def categorize_error(message: str) -> str:
    """Categorize an error message into a bucket."""
    for name, pattern in CATEGORY_PATTERNS:
        if pattern.search(message):
            return name
    return "uncategorized"


# ---------------------------------------------------------------------------
# Deterministic analysis
# ---------------------------------------------------------------------------


def load_report(path: Path) -> dict:
    with open(path) as f:
        return json.load(f)


def analyze_deterministic(data: dict) -> str:
    """Produce a markdown report from the JSON diff data."""
    lines = []
    w = lines.append

    summary = data["summary"]
    files = data["files"]

    # --- Header ---
    w("# V1 vs V2 Diagnostics Diff Report\n")
    w("## Summary\n")
    w(f"| Status | Count | Description |")
    w(f"|--------|-------|-------------|")
    w(f"| OK_CLEAN | {summary['ok_clean']} | Both compilers agree: no errors |")
    w(f"| HAS_BOTH | {summary['has_both']} | Both produce errors (need comparison) |")
    w(f"| V2_MISSING | **{summary['v2_missing']}** | V2 misses errors V1 expected (regressions) |")
    w(f"| V2_NEW | {summary['v2_new']} | V2 finds errors V1 didn't |")
    w(f"| V2_PANIC | {summary['v2_panic']} | V2 crashed |")
    w(f"| **Total** | **{summary['total_files']}** | |")
    w("")

    # --- V2_MISSING: regressions grouped by error category ---
    missing = [f for f in files if f["status"] == "V2_MISSING"]
    if missing:
        w("## V2_MISSING Regressions (by error category)\n")
        w("These are errors V1 caught but V2 does not. Grouped by error type to help")
        w("identify which compiler2 features need work.\n")

        # Categorize every error across all missing files
        category_files = defaultdict(list)  # category -> [(file, [errors])]
        for f in missing:
            file_cats = defaultdict(list)
            for err in f["v1_errors"]:
                cat = categorize_error(err)
                file_cats[cat].append(err)
            for cat, errs in file_cats.items():
                category_files[cat].append((f["file"], errs))

        # Sort categories by total error count (descending)
        cat_totals = {
            cat: sum(len(errs) for _, errs in entries)
            for cat, entries in category_files.items()
        }
        for cat in sorted(cat_totals, key=lambda c: -cat_totals[c]):
            total = cat_totals[cat]
            entries = category_files[cat]
            w(f"### `{cat}` ({total} errors across {len(entries)} files)\n")
            for filepath, errs in sorted(entries, key=lambda x: x[0]):
                w(f"- **{filepath}** ({len(errs)} errors)")
                for err in errs[:3]:
                    w(f"  - `{err[:120]}`")
                if len(errs) > 3:
                    w(f"  - ... and {len(errs) - 3} more")
            w("")

        # --- Summary table by category ---
        w("### Category summary\n")
        w("| Category | Errors | Files | Priority |")
        w("|----------|--------|-------|----------|")
        for cat in sorted(cat_totals, key=lambda c: -cat_totals[c]):
            total = cat_totals[cat]
            nfiles = len(category_files[cat])
            # Heuristic priority
            if cat in ("cycle_detection", "duplicate_name", "type_mismatch", "unknown_type"):
                priority = "P0 - core"
            elif cat in ("config_validation", "client_validation", "generator_validation"):
                priority = "P1 - config"
            elif cat in ("jinja_warning", "constraint_validation"):
                priority = "P1 - jinja/templates"
            elif cat in ("syntax_error", "reserved_keyword"):
                priority = "P2 - syntax"
            else:
                priority = "P2 - other"
            w(f"| {cat} | {total} | {nfiles} | {priority} |")
        w("")

    # --- HAS_BOTH: comparison ---
    has_both = [f for f in files if f["status"] == "HAS_BOTH"]
    if has_both:
        w("## HAS_BOTH Files (error count comparison)\n")
        w("Both compilers produce errors. Comparing counts to spot divergences.\n")
        w("| File | V1 errors | V2 errors | Delta | Assessment |")
        w("|------|-----------|-----------|-------|------------|")

        for f in sorted(has_both, key=lambda x: x["file"]):
            v1 = f["v1_error_count"]
            v2 = f["v2_error_count"]
            delta = v2 - v1
            if delta == 0:
                assessment = "count matches"
            elif delta > 0:
                assessment = f"V2 finds {delta} more"
            else:
                assessment = f"**V2 misses {-delta}**"
            sign = "+" if delta > 0 else ""
            w(f"| {f['file']} | {v1} | {v2} | {sign}{delta} | {assessment} |")
        w("")

        # Flag files where V2 finds fewer errors
        v2_fewer = [f for f in has_both if f["v2_error_count"] < f["v1_error_count"]]
        if v2_fewer:
            w("### HAS_BOTH files where V2 finds fewer errors\n")
            w("These may contain partial regressions (V2 catches some but not all V1 errors).\n")
            for f in sorted(v2_fewer, key=lambda x: x["file"]):
                w(f"#### {f['file']}\n")
                w(f"V1 ({f['v1_error_count']} errors):")
                for err in f["v1_errors"]:
                    w(f"- `{err[:120]}`")
                w(f"\nV2 ({f['v2_error_count']} errors):")
                for err in f["v2_errors"]:
                    w(f"- `{err[:120]}`")
                w("")

    # --- V2_NEW ---
    v2_new = [f for f in files if f["status"] == "V2_NEW"]
    if v2_new:
        w("## V2_NEW (V2 finds errors V1 didn't)\n")
        w("These are likely intentional syntax changes in V2 or stricter parsing.\n")
        for f in sorted(v2_new, key=lambda x: x["file"]):
            w(f"- **{f['file']}** ({f['v2_error_count']} errors)")
            for err in f["v2_errors"][:3]:
                w(f"  - `{err[:120]}`")
            if len(f["v2_errors"]) > 3:
                w(f"  - ... and {len(f['v2_errors']) - 3} more")
        w("")

    # --- Actionable next steps ---
    w("## Recommended next steps\n")
    if missing:
        top_cats = sorted(cat_totals, key=lambda c: -cat_totals[c])[:5]
        w("1. **Triage by category** - The biggest gaps are:")
        for i, cat in enumerate(top_cats, 1):
            w(f"   {i}. `{cat}` ({cat_totals[cat]} errors, {len(category_files[cat])} files)")
        w("")
        w("2. **Split the work** - Each category likely maps to a specific compiler2 subsystem:")
        w("   - `config_validation` / `client_validation` / `generator_validation` → config/client lowering")
        w("   - `cycle_detection` → TIR cycle detection pass")
        w("   - `jinja_warning` / `constraint_validation` → Jinja template type checking")
        w("   - `type_mismatch` / `unknown_type` → TIR type inference")
        w("   - `duplicate_name` / `duplicate_attribute` → HIR validation")
        w("")
        w("3. **Check HAS_BOTH files** with `V2 misses` for partial regressions")
        w("")
        w("4. **V2_NEW files** are likely fine (stricter V2 parsing) — review to confirm")
    w("")

    return "\n".join(lines)


# ---------------------------------------------------------------------------
# AI-assisted analysis
# ---------------------------------------------------------------------------


def analyze_with_ai(data: dict, report_path: Path) -> str:
    """Use Claude to do semantic comparison of diffs."""
    try:
        import anthropic
    except ImportError:
        print("ERROR: 'anthropic' package not installed. Run: pip install anthropic", file=sys.stderr)
        sys.exit(1)

    api_key = os.environ.get("ANTHROPIC_API_KEY")
    if not api_key:
        print("ERROR: ANTHROPIC_API_KEY not set.", file=sys.stderr)
        sys.exit(1)

    client = anthropic.Anthropic(api_key=api_key)

    # Focus AI analysis on the interesting cases
    files_to_analyze = []

    # V2_MISSING: ask if it's a real regression or intentional
    for f in data["files"]:
        if f["status"] == "V2_MISSING":
            # Read the actual .baml source
            source_path = V1_ROOT / f["file"]
            source = source_path.read_text() if source_path.exists() else "(file not found)"
            files_to_analyze.append({
                "file": f["file"],
                "status": f["status"],
                "source": source,
                "v1_errors": f["v1_errors"],
                "v2_errors": f["v2_errors"],
            })

    # HAS_BOTH where counts differ
    for f in data["files"]:
        if f["status"] == "HAS_BOTH" and f["v1_error_count"] != f["v2_error_count"]:
            source_path = V1_ROOT / f["file"]
            source = source_path.read_text() if source_path.exists() else "(file not found)"
            files_to_analyze.append({
                "file": f["file"],
                "status": f["status"],
                "source": source,
                "v1_errors": f["v1_errors"],
                "v2_errors": f["v2_errors"],
            })

    # Process in batches of 10
    BATCH_SIZE = 10
    all_verdicts = []

    for i in range(0, len(files_to_analyze), BATCH_SIZE):
        batch = files_to_analyze[i : i + BATCH_SIZE]
        batch_num = i // BATCH_SIZE + 1
        total_batches = (len(files_to_analyze) + BATCH_SIZE - 1) // BATCH_SIZE
        print(f"  Analyzing batch {batch_num}/{total_batches} ({len(batch)} files)...")

        prompt = build_analysis_prompt(batch)
        response = client.messages.create(
            model="claude-sonnet-4-20250514",
            max_tokens=4096,
            messages=[{"role": "user", "content": prompt}],
        )

        verdicts_text = response.content[0].text
        all_verdicts.append(verdicts_text)

    # Append AI analysis to the report
    ai_section = "\n## AI-Assisted Analysis\n\n"
    ai_section += "The following analysis was generated by Claude to provide semantic\n"
    ai_section += "comparison of V1 vs V2 diagnostic differences.\n\n"
    for v in all_verdicts:
        ai_section += v + "\n\n"

    # Read existing report and append
    existing = report_path.read_text() if report_path.exists() else ""
    return existing + ai_section


def build_analysis_prompt(batch: list[dict]) -> str:
    """Build a prompt for Claude to analyze a batch of diff files."""
    files_section = ""
    for f in batch:
        # Truncate source to keep prompt reasonable
        source = f["source"][:2000] + ("..." if len(f["source"]) > 2000 else "")
        files_section += f"""
### {f["file"]} (status: {f["status"]})

BAML source:
```baml
{source}
```

V1 errors: {json.dumps(f["v1_errors"], indent=2)}
V2 errors: {json.dumps(f["v2_errors"], indent=2)}

---
"""

    return f"""You are analyzing diagnostic differences between two versions of the BAML compiler.

For each file below, classify the difference as one of:
- **REAL_REGRESSION**: V2 is missing a diagnostic that users need. Say what's missing.
- **PARTIAL_REGRESSION**: V2 catches some errors but misses others. List what's missing.
- **INTENTIONAL_CHANGE**: The diagnostic was removed on purpose (syntax change, different approach).
- **COSMETIC**: Same errors detected, just different messages or rendering.
- **NEEDS_INVESTIGATION**: Can't determine from the data alone.

Also assign a priority:
- **P0**: Users will hit this in normal usage (type errors, missing validations on common patterns)
- **P1**: Users may hit this (edge cases, config validation)
- **P2**: Unlikely to affect users (deprecated features, obscure patterns)

Respond in markdown with a table, then brief notes for each file.

{files_section}

Respond with:
1. A summary table: | File | Verdict | Priority | Notes |
2. Brief explanation for each non-obvious verdict.
"""


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# LSP diff analysis
# ---------------------------------------------------------------------------


def analyze_lsp_deterministic(data: dict) -> str:
    """Produce a markdown report from the LSP diff JSON data."""
    lines = []
    w = lines.append

    summary = data["summary"]
    files = data["files"]

    w("# LSP V1 vs V2 Diagnostics Diff Report\n")
    w("Compares diagnostics expectations between `baml_lsp_actions_tests` (compiler v1)")
    w("and `baml_lsp2_actions_tests` (compiler2), and checks actual compiler2 output.\n")

    w("## Summary\n")
    w("| Metric | Count | Description |")
    w("|--------|-------|-------------|")
    w(f"| Total files | {summary['total_files']} | LSP syntax test files |")
    w(f"| Expectations match | {summary['expectations_match']} | V1 and V2 expect the same diagnostics |")
    w(f"| Expectations differ | {summary['expectations_differ']} | V2 expectations were updated for compiler2 |")
    w(f"| V2 passing | {summary['v2_passing']} | Actual output matches V2 expectations |")
    w(f"| **V2 failing** | **{summary['v2_failing']}** | **Actual output doesn't match V2 expectations** |")
    w(f"| V2 panic | {summary['v2_panic']} | Compiler2 crashed |")
    w("")

    # Files where expectations were updated (compiler2 behavior changed)
    updated = [f for f in files if not f["expectations_match"]]
    if updated:
        w("## Files with updated expectations\n")
        w("These files have different expected diagnostics between v1 and v2 LSP tests,")
        w("meaning compiler2 intentionally changed behavior here.\n")
        w("| File | V2 passes? | Status |")
        w("|------|-----------|--------|")
        for f in sorted(updated, key=lambda x: x["file"]):
            pass_str = "yes" if f["v2_passes"] else "**no**"
            w(f"| {f['file']} | {pass_str} | {f['status']} |")
        w("")

    # Files where v2 is failing
    failing = [f for f in files if not f["v2_passes"] and not f["status"].startswith("PANIC")]
    if failing:
        w("## Failing files (actual != expected)\n")
        w("Compiler2 output doesn't match the V2 expectations for these files.\n")
        for f in sorted(failing, key=lambda x: x["file"]):
            w(f"### {f['file']}\n")
            w(f"**Status:** `{f['status']}`\n")
            if not f["expectations_match"]:
                w("V1 expected:")
                w(f"```\n{f['v1_expected'][:500]}\n```\n")
            w("V2 expected:")
            w(f"```\n{f['v2_expected'][:500]}\n```\n")
            w("V2 actual:")
            w(f"```\n{f['v2_actual'][:500]}\n```\n")

    # Panics
    panics = [f for f in files if f["status"].startswith("PANIC")]
    if panics:
        w("## Panics\n")
        for f in panics:
            w(f"- **{f['file']}**: `{f['status']}`")
        w("")

    w("## Next steps\n")
    w(f"1. **{summary['v2_failing']} files need attention** — actual compiler2 output differs from expected")
    w(f"2. **{summary['expectations_differ']} files had expectations updated** — review these for correctness")
    w(f"3. **{summary['v2_passing']} files pass** — no action needed")
    w("")

    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--mode", choices=["engine", "lsp", "both"], default="both",
                       help="Which diff to analyze: 'engine' (V1 engine files), 'lsp' (LSP test files), or 'both'")
    parser.add_argument("--input", type=Path, default=None, help="Path to JSON diff report (overrides --mode)")
    parser.add_argument("--output", type=Path, default=None, help="Path to output markdown report (overrides --mode)")
    parser.add_argument("--with-ai", action="store_true", help="Enable AI-assisted analysis (requires ANTHROPIC_API_KEY)")
    args = parser.parse_args()

    modes = []
    if args.input:
        # Explicit input — just run that
        modes.append(("custom", args.input, args.output or args.input.with_suffix(".md")))
    elif args.mode == "engine":
        modes.append(("engine", DEFAULT_INPUT, DEFAULT_OUTPUT))
    elif args.mode == "lsp":
        modes.append(("lsp", LSP_INPUT, LSP_OUTPUT))
    else:  # both
        modes.append(("engine", DEFAULT_INPUT, DEFAULT_OUTPUT))
        modes.append(("lsp", LSP_INPUT, LSP_OUTPUT))

    for mode_name, input_path, output_path in modes:
        if not input_path.exists():
            print(f"SKIP: {input_path} not found (run the Rust test first)", file=sys.stderr)
            if mode_name == "engine":
                print("  cargo test -p baml_tests --lib v1_diagnostics_diff -- --nocapture", file=sys.stderr)
            elif mode_name == "lsp":
                print("  cargo test -p baml_tests --lib lsp_diagnostics_diff -- --nocapture", file=sys.stderr)
            continue

        print(f"\n[{mode_name}] Reading {input_path}...")
        data = load_report(input_path)

        if mode_name == "lsp":
            print(f"[{mode_name}] Running LSP analysis...")
            report = analyze_lsp_deterministic(data)
            output_path.write_text(report)
            print(f"[{mode_name}] Report written to {output_path}")
            s = data["summary"]
            print(f"[{mode_name}] Done. {s['v2_failing']} failing, {s['v2_passing']} passing, {s['expectations_differ']} updated.")
        else:
            print(f"[{mode_name}] Running deterministic analysis...")
            report = analyze_deterministic(data)
            output_path.write_text(report)
            print(f"[{mode_name}] Report written to {output_path}")

            if args.with_ai:
                print(f"[{mode_name}] Running AI-assisted analysis...")
                full_report = analyze_with_ai(data, output_path)
                output_path.write_text(full_report)
                print(f"[{mode_name}] AI analysis appended to {output_path}")

            s = data["summary"]
            print(f"[{mode_name}] Done. {s['v2_missing']} regressions, {s['has_both']} need comparison, {s['ok_clean']} clean.")


if __name__ == "__main__":
    main()
