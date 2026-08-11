#!/usr/bin/env python3
"""C0 gate validation for the CANONICAL design set.

Checks:
1. Every relative markdown link in CANONICAL/**.md resolves to an existing file.
2. Every anchor link (#fragment) resolves to a real heading in the target file.
3. Every file in CANONICAL/archive (recursively, *.md) has a disposition row in
   design/11-source-map.md.
4. Terminology bans: stale terms must not be stated as current outside the
   archive (they may appear in correction tables that explicitly mark them
   wrong; we report occurrences for manual review with context).
"""
import os
import re
import sys
import unicodedata

ROOT = "/root/dev/baml/CANONICAL"
ARCHIVE = os.path.join(ROOT, "archive")
SOURCE_MAP = os.path.join(ROOT, "design", "11-source-map.md")

link_re = re.compile(r"\[([^\]]*)\]\(([^)\s]+)\)")
heading_re = re.compile(r"^#{1,6}\s+(.*)$", re.M)

def github_anchor(text: str) -> str:
    text = re.sub(r"[*`~]", "", text)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = unicodedata.normalize("NFKD", text)
    text = text.lower().strip()
    text = re.sub(r"[^\w\- ]", "", text)
    text = text.replace(" ", "-")
    return text

def md_files(base):
    for dirpath, _dirnames, filenames in os.walk(base):
        for f in filenames:
            if f.endswith(".md"):
                yield os.path.join(dirpath, f)

def collect_anchors(path):
    try:
        text = open(path, encoding="utf-8").read()
    except OSError:
        return set()
    # strip fenced code blocks so headings inside fences don't count
    text = re.sub(r"~~~.*?~~~|```.*?```", "", text, flags=re.S)
    return {github_anchor(m) for m in heading_re.findall(text)}

failures = []
warnings = []

# --- 1 & 2: link resolution ---
anchor_cache = {}
for path in sorted(md_files(ROOT)):
    text = open(path, encoding="utf-8").read()
    stripped = re.sub(r"~~~.*?~~~|```.*?```", "", text, flags=re.S)
    for label, target in link_re.findall(stripped):
        if target.startswith(("http://", "https://", "mailto:")):
            continue
        frag = None
        if "#" in target:
            target, frag = target.split("#", 1)
        if target == "":
            dest = path
        else:
            dest = os.path.normpath(os.path.join(os.path.dirname(path), target))
        if not os.path.exists(dest):
            failures.append(f"BROKEN LINK  {os.path.relpath(path, ROOT)} -> {target}")
            continue
        if frag is not None and dest.endswith(".md"):
            if dest not in anchor_cache:
                anchor_cache[dest] = collect_anchors(dest)
            if frag.lower() not in anchor_cache[dest]:
                failures.append(
                    f"BROKEN ANCHOR {os.path.relpath(path, ROOT)} -> {os.path.relpath(dest, ROOT)}#{frag}"
                )

# --- 3: archive dispositions ---
smap = open(SOURCE_MAP, encoding="utf-8").read()
for path in sorted(md_files(ARCHIVE)):
    name = os.path.basename(path)
    if name == "README.md":
        continue
    if name not in smap:
        failures.append(f"NO DISPOSITION for archive doc: {os.path.relpath(path, ROOT)}")

# --- 4: stale-terminology scan outside archive ---
stale_terms = {
    r"meta\.bamlmeta": "session metadata file is session.bamlmeta",
    r"\.bpk1\b": "packs are .bamlpack",
    r"\.bpki\b": "indexes are .bamlpack.idx",
    r"index\.jsonl": "no root index exists; reader scans boundary meta",
    r"StudioQueryV1": "superseded surface (allowed only as historical mention)",
    r"\bchDB\b": "superseded local engine (allowed only as historical mention)",
}
for path in sorted(md_files(ROOT)):
    rel = os.path.relpath(path, ROOT)
    if rel.startswith("archive"):
        continue
    text = open(path, encoding="utf-8").read()
    for pat, why in stale_terms.items():
        for m in re.finditer(pat, text):
            line_no = text.count("\n", 0, m.start()) + 1
            line = text.splitlines()[line_no - 1].strip()
            warnings.append(f"TERM {rel}:{line_no}: /{pat}/ ({why})\n    {line[:160]}")

print("== C0 validation ==")
if failures:
    print(f"\n{len(failures)} FAILURES:")
    for f in failures:
        print("  " + f)
else:
    print("\nNo link/anchor/disposition failures.")
if warnings:
    print(f"\n{len(warnings)} terminology mentions to review:")
    for w in warnings:
        print("  " + w)
sys.exit(1 if failures else 0)
