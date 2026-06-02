"""One-off generator: build docs/reference.md from the in-code docstrings/JSDoc.

Walks the Python sources (libs/bench_core + services, via AST) and the TypeScript
sources (convex + ui, via regex) and emits a single consolidated API reference -
every function/method/class with its one-line summary.
"""
from __future__ import annotations

import ast
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent          # tools/baml-bench/docs
ROOT = HERE.parent                               # tools/baml-bench
OUT = HERE / "reference.md"


def first_line(doc: str | None) -> str:
    """Collapse a docstring's first paragraph to one line, or an undocumented placeholder.

    Takes everything up to the first blank line (so a summary that wraps across
    several physical lines reads as one full sentence) and normalizes whitespace.

    Args:
        doc: The raw docstring/JSDoc summary text, or None.

    Returns:
        The first paragraph as a single whitespace-normalized line, or
        "_(undocumented)_" when there is no docstring.
    """
    if not doc:
        return "_(undocumented)_"
    para: list[str] = []
    for ln in doc.strip().splitlines():
        if not ln.strip():
            break
        para.append(ln.strip())
    return " ".join(" ".join(para).split())


def sig(args: ast.arguments) -> str:
    """Render a function's parameter names as a signature string.

    Drops ``self``/``cls`` and prefixes ``*args``/``**kwargs`` appropriately.

    Args:
        args: The AST arguments node of a function or method.

    Returns:
        A comma-separated parameter list (names only, no types or defaults).
    """
    names = [a.arg for a in args.posonlyargs + args.args if a.arg not in ("self", "cls")]
    if args.vararg:
        names.append("*" + args.vararg.arg)
    names += [a.arg for a in args.kwonlyargs]
    if args.kwarg:
        names.append("**" + args.kwarg.arg)
    return ", ".join(names)


def py_section(path: Path) -> list[str]:
    """Build the markdown reference lines for one Python module.

    Lists each top-level function and class (with its methods), each annotated
    with its one-line docstring summary.

    Args:
        path: The .py file to document.

    Returns:
        Markdown lines: a section header plus one bullet per symbol.
    """
    tree = ast.parse(path.read_text(encoding="utf-8"))
    rel = path.relative_to(ROOT)
    lines = [f"### `{rel}`", ""]
    for node in tree.body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            lines.append(f"- **`{node.name}({sig(node.args)})`** - {first_line(ast.get_docstring(node))}")
        elif isinstance(node, ast.ClassDef):
            lines.append(f"- **`class {node.name}`** - {first_line(ast.get_docstring(node))}")
            for m in node.body:
                if isinstance(m, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    lines.append(f"    - `{m.name}({sig(m.args)})` - {first_line(ast.get_docstring(m))}")
    lines.append("")
    return lines


# A JSDoc block immediately followed by an export (function/const/class/type/interface,
# optionally default/async). Group 1 is the JSDoc body, group 2 the exported name. The
# tempered ``(?:(?!\*/).)*?`` keeps the block to a single comment so a non-exported
# helper's JSDoc can't be mis-paired with the next export.
EXPORT = re.compile(
    r"/\*\*((?:(?!\*/).)*?)\*/\s*export\s+(?:default\s+)?(?:async\s+)?"
    r"(?:function|const|class|type|interface)\s+(\w+)",
    re.S,
)


def ts_section(path: Path) -> list[str]:
    """Build the markdown reference lines for one TypeScript module.

    Extracts each exported symbol (function/const/class/type/interface) preceded by
    a JSDoc block, using the block's first non-tag line as the summary.

    Args:
        path: The .ts/.tsx file to document.

    Returns:
        Markdown lines: a section header plus one bullet per exported symbol.
    """
    text = path.read_text(encoding="utf-8")
    rel = path.relative_to(ROOT)
    lines = [f"### `{rel}`", ""]
    for block, name in EXPORT.findall(text):
        body = [l.strip().lstrip("*").strip() for l in block.splitlines()]
        summary = next((l for l in body if l and not l.startswith("@")), "")
        lines.append(f"- **`{name}`** - {summary}")
    lines.append("")
    return lines


def main() -> None:
    """Generate docs/reference.md from the Python (bench_core + services) and
    TypeScript (convex + ui) sources.

    Walks each source tree, assembles the consolidated reference grouped by area,
    and writes it to ``reference.md`` next to this script.
    """
    out = [
        "# API reference",
        "",
        "Consolidated index of every function, method, and class (private,",
        "underscore-prefixed helpers included), with a one-line summary. Generated",
        "from the in-code docstrings / JSDoc (run",
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
    out += ["## `services` (Python)", ""]
    for p in sorted((ROOT / "services").rglob("*.py")):
        if p.name == "__init__.py":
            continue
        out += py_section(p)
    out += ["## `convex` (TypeScript)", ""]
    for p in sorted((ROOT / "convex").glob("*.ts")):
        sec = ts_section(p)
        if len(sec) > 3:  # has at least one export
            out += sec
    out += ["## `ui` (TypeScript)", ""]
    ui_files = sorted(list((ROOT / "ui" / "app").rglob("*.ts")) +
                      list((ROOT / "ui" / "app").rglob("*.tsx")))
    for p in ui_files:
        sec = ts_section(p)
        if len(sec) > 3:  # has at least one documented export
            out += sec
    OUT.write_text("\n".join(out) + "\n")
    print(f"wrote {OUT} ({len(out)} lines)")


if __name__ == "__main__":
    main()
