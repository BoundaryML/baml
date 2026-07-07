#!/usr/bin/env python3
"""Regenerate baml_src/catalog_embed.baml from design-principles.md."""
import pathlib
root = pathlib.Path(__file__).resolve().parent.parent
src = (root / 'design-principles.md').read_text()
assert '"##' not in src, 'catalog now contains \"## — bump the raw-string hash count'
out = root / 'baml_src' / 'catalog_embed.baml'
out.write_text(
    '// GENERATED — the design-principles.md catalog embedded at pack time so the\n'
    '// packed `baml-bench` binary is self-contained. Regenerate with:\n'
    '//   python3 scripts/embed_catalog.py\n'
    '// Double-hash raw string: the catalog contains `"#` but no `"##`.\n\n'
    'function embedded_catalog() -> string {\n    ##"' + src + '"##\n}\n'
)
print(f'wrote {out} ({len(src)} chars)')
