"""One-off generator: build docs/reference.md from the in-code docstrings/JSDoc.

Walks libs/bench_core/*.py (via AST) and convex/*.ts (via regex) and emits a single
consolidated API reference — every function/method/class with its one-line summary.
"""
from __future__ import annotations

import ast
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent          # tools/baml-bench/docs
ROOT = HERE.parent                               # tools/baml-bench
OUT = HERE / "reference.md"


def first_line(doc: str | None) -> str:
    if not doc:
        return "_(undocumented)_"
    return " ".join(doc.strip().splitlines()[0].split())


def sig(args: ast.arguments) -> str:
    names = [a.arg for a in args.posonlyargs + args.args if a.arg not in ("self", "cls")]
    if args.vararg:
        names.append("*" + args.vararg.arg)
    names += [a.arg for a in args.kwonlyargs]
    if args.kwarg:
        names.append("**" + args.kwarg.arg)
    return ", ".join(names)


def py_section(path: Path) -> list[str]:
    tree = ast.parse(path.read_text())
    rel = path.relative_to(ROOT)
    lines = [f"### `{rel}`", ""]
    for node in tree.body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            lines.append(f"- **`{node.name}({sig(node.args)})`** — {first_line(ast.get_docstring(node))}")
        elif isinstance(node, ast.ClassDef):
            lines.append(f"- **`class {node.name}`** — {first_line(ast.get_docstring(node))}")
            for m in node.body:
                if isinstance(m, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    lines.append(f"    - `{m.name}({sig(m.args)})` — {first_line(ast.get_docstring(m))}")
    lines.append("")
    return lines


JSDOC = re.compile(r"/\*\*(.*?)\*/\s*export const (\w+)\s*=", re.S)


def ts_section(path: Path) -> list[str]:
    text = path.read_text()
    rel = path.relative_to(ROOT)
    lines = [f"### `{rel}`", ""]
    for block, name in JSDOC.findall(text):
        body = [l.strip().lstrip("*").strip() for l in block.splitlines()]
        summary = next((l for l in body if l and not l.startswith("@")), "")
        lines.append(f"- **`{name}`** — {summary}")
    lines.append("")
    return lines


def main() -> None:
    out = [
        "# API reference",
        "",
        "Consolidated index of every public function, method, and class, with a",
        "one-line summary. Generated from the in-code docstrings / JSDoc (run",
        "`python docs/_gen_reference.py` to refresh). Full parameter/return detail",
        "lives in the docstrings themselves.",
        "",
        "## `libs/bench_core` (Python)",
        "",
    ]
    for p in sorted((ROOT / "libs" / "bench_core").glob("*.py")):
        if p.name == "__init__.py":
            continue
        out += py_section(p)
    out += ["## `convex` (TypeScript)", ""]
    for p in sorted((ROOT / "convex").glob("*.ts")):
        sec = ts_section(p)
        if len(sec) > 3:  # has at least one export
            out += sec
    OUT.write_text("\n".join(out) + "\n")
    print(f"wrote {OUT} ({len(out)} lines)")


if __name__ == "__main__":
    main()
