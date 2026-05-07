#!/usr/bin/env python3
"""drift_check.py — verifies that the .baml files under
cross_lang/workloads/baml/ are byte-identical to the inline workload
strings in runtime_benchmark.rs (Suite A1).

When this fails: someone edited one side without the other. Either
update the inline string in runtime_benchmark.rs or regenerate the
.baml file from this script's helpers, but never both differently.

Exits 0 on success, 1 on drift, 2 on hard error (file not found, parse
failure).
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

CROSS_LANG_DIR = Path(__file__).resolve().parent.parent  # cross_lang/
BENCHES_DIR = CROSS_LANG_DIR.parent  # benches/
RUNTIME_RS = BENCHES_DIR / "runtime_benchmark.rs"
BAML_DIR = CROSS_LANG_DIR / "workloads" / "baml"

# Mapping: BAML file stem → bench fn name in runtime_benchmark.rs.
INLINE_WORKLOADS = {
    "fib_20": "vm_fib_20",
    "loop_500k": "vm_loop_500k",
    "string_concat_5k": "vm_string_concat_5k",
    "array_push_50k": "vm_array_push_50k",
    "array_iter_10k": "vm_array_iter_10k",
    "class_create_50k": "vm_class_create_50k",
    "field_access_50k": "vm_field_access_50k",
    "nested_loop": "vm_nested_loop",
    "mixed_ops": "vm_mixed_ops",
    "closure_call_50k": "vm_closure_call_50k",
    "hello_world": "e2e_hello_world",
    "arithmetic": "e2e_arithmetic",
    "fib_20_e2e": "e2e_fib_20",
    "class_and_loop": "e2e_class_and_loop",
}

# Workloads synthesized by helpers in runtime_benchmark.rs.
SYNTH_WORKLOADS = {
    "call_chain_100_x_5k": ("generate_call_chain", (100, 5000)),
    "one_hundred_functions": ("generate_n_functions", (100,)),
}


def generate_call_chain(depth: int, iterations: int) -> str:
    """Re-implementation of runtime_benchmark.rs::generate_call_chain."""
    s = ""
    for i in range(depth - 1):
        s += f"function f{i}(n: int) -> int {{ f{i+1}(n + 1) }}\n"
    s += f"function f{depth-1}(n: int) -> int {{ n }}\n"
    s += (
        "function main() -> int {\n"
        "  let s = 0;\n"
        f"  for (let i = 0; i < {iterations}; i += 1) {{\n"
        "    s += f0(0);\n"
        "  };\n"
        "  return s;\n"
        "}\n"
    )
    return s


def generate_n_functions(n: int) -> str:
    """Re-implementation of runtime_benchmark.rs::generate_n_functions."""
    s = ""
    for i in range(n):
        s += f"function f{i}(x: int) -> int {{ x + {i} }}\n"
    s += "function main() -> int {\n  let s = 0;\n"
    for i in range(n):
        s += f"  s += f{i}(1);\n"
    s += "  return s;\n}\n"
    return s


SYNTH_FNS = {
    "generate_call_chain": generate_call_chain,
    "generate_n_functions": generate_n_functions,
}


def extract_inline(rust_src: str, fn_name: str) -> str:
    """Pull the r#"..."# raw string immediately following the bench fn.

    runtime_benchmark.rs benches all look like:

        fn vm_xxx(bencher: Bencher) {
            bencher.with_inputs(|| {
                let (db, engine) = compile_source(
                    r#"
        ...
        "#,
                );
                ...

    or for e2e:
            bencher.bench(|| {
                let (db, engine) = compile_source(r#"function main() -> ..."#);

    We find the first `r#"..."#` after the fn definition and extract.
    """
    fn_pat = re.compile(rf"\bfn\s+{re.escape(fn_name)}\s*\(", re.MULTILINE)
    m = fn_pat.search(rust_src)
    if not m:
        raise ValueError(f"function {fn_name} not found in runtime_benchmark.rs")
    after = rust_src[m.end():]
    raw_pat = re.compile(r'r#"(.*?)"#', re.DOTALL)
    rm = raw_pat.search(after)
    if not rm:
        raise ValueError(f"no raw string after {fn_name}")
    return rm.group(1)


def normalize(s: str) -> str:
    """The inline strings often start/end with newlines; .baml files don't.
    Strip a single leading and single trailing newline so we compare the
    content fairly."""
    if s.startswith("\n"):
        s = s[1:]
    if not s.endswith("\n"):
        s = s + "\n"
    return s


def main() -> int:
    if not RUNTIME_RS.is_file():
        print(f"runtime_benchmark.rs not found at {RUNTIME_RS}", file=sys.stderr)
        return 2
    if not BAML_DIR.is_dir():
        print(f"workloads/baml/ not found at {BAML_DIR}", file=sys.stderr)
        return 2

    rust_src = RUNTIME_RS.read_text()
    failures: list[str] = []

    for stem, fn_name in INLINE_WORKLOADS.items():
        baml_path = BAML_DIR / f"{stem}.baml"
        if not baml_path.is_file():
            failures.append(f"missing: {baml_path.relative_to(BENCHES_DIR.parent)}")
            continue
        try:
            inline = extract_inline(rust_src, fn_name)
        except ValueError as e:
            failures.append(f"{stem}: {e}")
            continue
        expected = normalize(inline)
        actual = baml_path.read_text()
        if expected != actual:
            failures.append(
                f"DRIFT: {stem}.baml ↔ runtime_benchmark.rs::{fn_name}\n"
                f"  expected (from rust):\n{indent(expected)}\n"
                f"  actual (.baml file):\n{indent(actual)}"
            )

    for stem, (synth_name, args) in SYNTH_WORKLOADS.items():
        baml_path = BAML_DIR / f"{stem}.baml"
        if not baml_path.is_file():
            failures.append(f"missing: {baml_path.relative_to(BENCHES_DIR.parent)}")
            continue
        expected = SYNTH_FNS[synth_name](*args)
        actual = baml_path.read_text()
        if expected != actual:
            failures.append(
                f"DRIFT: {stem}.baml ↔ {synth_name}{args}\n"
                f"  rerun this script after regenerating the .baml file."
            )

    if failures:
        print("Suite A2 drift check FAILED:", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    print(f"Suite A2 drift check OK ({len(INLINE_WORKLOADS) + len(SYNTH_WORKLOADS)} workloads)")
    return 0


def indent(s: str, prefix: str = "    ") -> str:
    return "\n".join(prefix + line for line in s.splitlines())


if __name__ == "__main__":
    sys.exit(main())
