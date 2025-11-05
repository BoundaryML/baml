#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "rich>=13",
# ]
# ///

"""Print a colorized table of `[lints.rust]` settings for all members of the Cargo workspace.

The script discovers workspace members via the root `Cargo.toml`, loads each manifest,
and lists configured Rust lints (e.g., `dead_code`, `mismatched_lifetime_syntaxes`) so
you can compare enforcement levels across crates.

┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┳━━━━━━━━━━━┳━━━━━━━━━━━━┳━━━━━━━━━━━━━━━━━━━━━━━━┳━━━━━━━━━━━━━━━━┳━━━━━━━━━━━━━━━━━┳━━━━━━━━━━━━━━━━━━┓
┃ package                               ┃ dead_code ┃ deprecated ┃ elided_named_lifetimes ┃ unused_imports ┃ unused_must_use ┃ unused_variables ┃
┡━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━╇━━━━━━━━━━━╇━━━━━━━━━━━━╇━━━━━━━━━━━━━━━━━━━━━━━━╇━━━━━━━━━━━━━━━━╇━━━━━━━━━━━━━━━━━╇━━━━━━━━━━━━━━━━━━┩
│ baml-ids/Cargo.toml                   │ deny      │            │ deny                   │ deny           │                 │ deny             │
│ baml-lib/ast/Cargo.toml               │ deny      │            │ deny                   │ allow          │                 │                  │
│ baml-lib/baml/Cargo.toml              │ allow     │            │ deny                   │ allow          │                 │ allow            │
│ baml-lib/baml-core/Cargo.toml         │ allow     │            │ deny                   │ allow          │                 │ allow            │
│ baml-lib/baml-log/Cargo.toml          │ deny      │            │ deny                   │ deny           │                 │ deny             │
│ baml-lib/baml-types/Cargo.toml        │ deny      │            │ deny                   │ deny           │                 │ deny             │
│ baml-lib/diagnostics/Cargo.toml       │ deny      │            │ deny                   │ deny           │                 │ deny             │
│ baml-lib/jinja/Cargo.toml             │ deny      │            │ deny                   │ deny           │                 │ deny             │
│ baml-lib/jinja-runtime/Cargo.toml     │ allow     │            │ deny                   │ deny           │                 │ deny             │
│ baml-lib/jsonish/Cargo.toml           │ allow     │            │ deny                   │ allow          │                 │ allow            │
│ baml-lib/llm-client/Cargo.toml        │ deny      │            │ deny                   │ deny           │                 │ deny             │
│ baml-lib/parser-database/Cargo.toml   │ allow     │            │ deny                   │ allow          │                 │ allow            │
│ baml-lib/prompt-parser/Cargo.toml     │ deny      │            │ deny                   │ deny           │                 │ deny             │
│ baml-lsp-types/Cargo.toml             │           │            │ deny                   │                │                 │                  │
│ baml-rpc/Cargo.toml                   │ deny      │            │ deny                   │ deny           │                 │ deny             │
│ baml-runtime/Cargo.toml               │ allow     │            │ deny                   │ allow          │                 │ allow            │
│ baml-schema-wasm/Cargo.toml           │ deny      │            │ deny                   │ deny           │                 │ deny             │
│ bstd/Cargo.toml                       │ deny      │            │ deny                   │ deny           │                 │ deny             │
│ cli/Cargo.toml                        │ deny      │            │ deny                   │ deny           │                 │ deny             │
│ language_client_python/Cargo.toml     │ deny      │            │ deny                   │ deny           │ deny            │ deny             │
│ language_client_typescript/Cargo.toml │ deny      │ allow      │ deny                   │ allow          │                 │ allow            │
│ language_server/Cargo.toml            │ allow     │            │ deny                   │ allow          │                 │ allow            │
│ playground-server/Cargo.toml          │ deny      │            │ deny                   │ deny           │                 │ deny             │
└───────────────────────────────────────┴───────────┴────────────┴────────────────────────┴────────────────┴─────────────────┴──────────────────┘
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any

try:  # Python 3.11+
    import tomllib  # type: ignore
except ModuleNotFoundError:  # pragma: no cover - fallback for older interpreters
    sys.stderr.write("Python 3.11+ required (missing tomllib)\n")
    sys.exit(1)

from rich.console import Console
from rich.table import Table
from rich.theme import Theme



def find_workspace_root(start: Path) -> Path:
    for candidate in (start, *start.parents):
        manifest = candidate / "Cargo.toml"
        if manifest.exists():
            with manifest.open("rb") as fh:
                data = tomllib.load(fh)
            if "workspace" in data:
                return candidate
    raise RuntimeError("Unable to locate Cargo workspace root")


def read_workspace_members(root: Path) -> list[Path]:
    manifest_path = root / "Cargo.toml"
    with manifest_path.open("rb") as fh:
        manifest = tomllib.load(fh)

    members = manifest.get("workspace", {}).get("members", [])
    resolved: set[Path] = set()

    for member in members:
        if any(ch in member for ch in "*?[]"):
            for match in root.glob(member):
                candidate = match / "Cargo.toml" if match.is_dir() else match
                if candidate.is_file() and candidate.name == "Cargo.toml":
                    resolved.add(candidate.resolve())
        else:
            candidate = (root / member).resolve()
            manifest_file = candidate / "Cargo.toml"
            if manifest_file.is_file():
                resolved.add(manifest_file.resolve())

    resolved.add((root / "Cargo.toml").resolve())

    return sorted(resolved)


def read_rust_lints(manifest_path: Path) -> dict[str, Any]:
    with manifest_path.open("rb") as fh:
        manifest = tomllib.load(fh)

    lints = manifest.get("lints", {}).get("rust")
    if not isinstance(lints, dict):
        return {}
    return lints


def format_value(value: Any) -> tuple[str, str]:
    level = None
    display: str

    if isinstance(value, str):
        display = value
        level = value.lower()
    elif isinstance(value, dict):
        parts = []
        for key, val in value.items():
            parts.append(f"{key}={val}")
            if key == "level" and isinstance(val, str):
                level = val.lower()
        display = ", ".join(parts)
    elif isinstance(value, bool):
        display = str(value).lower()
    elif isinstance(value, (int, float)):
        display = str(value)
    elif isinstance(value, list):
        display = ", ".join(map(str, value))
    else:
        display = repr(value)

    style = {
        "allow": "green",
        "warn": "yellow",
        "deny": "bold red",
        "forbid": "bold red",
        "deny(warn)": "red",
    }.get(level, "white")

    return display, style


def main() -> None:
    console = Console(theme=Theme({"path": "bold cyan"}))

    start = Path.cwd()
    root = find_workspace_root(start)
    manifests = read_workspace_members(root)

    rows: list[tuple[str, dict[str, tuple[str, str]]]] = []
    lint_names: set[str] = set()

    for manifest in manifests:
        lints = read_rust_lints(manifest)
        if not lints:
            continue

        formatted: dict[str, tuple[str, str]] = {}
        for name, value in lints.items():
            display, style = format_value(value)
            formatted[name] = (display, style)
            lint_names.add(name)

        rel_path = str(manifest.relative_to(root))
        rows.append((rel_path, formatted))

    if not rows:
        console.print("[bold yellow]No [lints.rust] sections found in workspace members.[/]")
        return

    sorted_lints = sorted(lint_names)
    table = Table(show_lines=False)
    table.add_column("package", style="path", overflow="fold")

    for lint_name in sorted_lints:
        table.add_column(lint_name, overflow="fold")

    for rel_path, formatted in rows:
        row_cells = [rel_path]
        for lint_name in sorted_lints:
            cell = formatted.get(lint_name)
            if cell is None:
                row_cells.append("")
            else:
                text, style = cell
                row_cells.append(f"[{style}]{text}[/]")
        table.add_row(*row_cells)

    console.print(table)


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:  # pragma: no cover - surface errors clearly
        Console().print(f"[bold red]Error:[/] {exc}")
        sys.exit(1)
