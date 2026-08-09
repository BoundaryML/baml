#!/usr/bin/env python3
"""Publish this BEP to beps.boundaryml.com as BEP-064.

Builds the pages payload from readme.md + pages/**, rewrites repo-relative
`.md` cross-references into app links (/beps/64/pages/<slug>), and PUTs.

Usage:
  BEPS_API_TOKEN=... python3 publish.py "edit note" [--new|--current]
"""
import json
import os
import re
import sys
import urllib.request

BASE = os.path.dirname(os.path.abspath(__file__))
BEP = 64
URL = "https://beps.boundaryml.com/api/agent/beps"


def read(p):
    with open(os.path.join(BASE, p)) as f:
        return f.read()


def title_of(content, fallback):
    m = re.search(r"^# (.+)$", content, re.M)
    return m.group(1).strip() if m else fallback


def slug_of(fname):
    return re.sub(r"^\d+_", "", fname.replace(".md", "")).replace("_", "-")


def collect():
    """(slug, title, content, parent, source_basename) for every page."""
    out = []

    def leaves(dirpath, parent):
        names = sorted(
            f for f in os.listdir(os.path.join(BASE, dirpath)) if re.match(r"\d+_.*\.md$", f)
        )
        titles = []
        for f in names:
            c = read(f"{dirpath}/{f}")
            t = title_of(c, f)
            titles.append((t, slug_of(f)))
            out.append([slug_of(f), t, c, parent, f])
        return titles

    def stub(title, children):
        kids = "\n".join(f"- [{t}](/beps/{BEP}/pages/{s})" for t, s in children)
        return f"# {title}\n\nPages in this section:\n\n{kids}\n"

    out.append(["introduction", "Introduction", None, None, None])
    i = len(out) - 1
    out[i][2] = stub("Introduction", leaves("pages/01_introduction", "introduction"))

    out.append(["guides", "Guides", None, None, None])
    gi = len(out) - 1
    sections = []
    for slug, title, d in [
        ("functions", "Functions", "pages/02_guides/01_functions"),
        ("specs-and-runners", "Specs and runners", "pages/02_guides/02_specs_and_runners"),
        ("clients", "Clients", "pages/02_guides/03_clients"),
    ]:
        out.append([slug, title, None, "guides", None])
        si = len(out) - 1
        out[si][2] = stub(title, leaves(d, slug))
        sections.append((title, slug))
    jc = read("pages/02_guides/04_the_journal.md")
    out.append(["the-journal", title_of(jc, "The journal"), jc, "guides", "04_the_journal.md"])
    sections.append((title_of(jc, "The journal"), "the-journal"))
    out[gi][2] = stub("Guides", sections)

    out.append(["how-to", "How-to guides", read("pages/03_how_to/readme.md"), None, "readme.md"])
    leaves("pages/03_how_to", "how-to")

    out.append(["reference", "Reference", None, None, None])
    ri = len(out) - 1
    out[ri][2] = stub("Reference", leaves("pages/04_reference", "reference"))

    out.append(["appendix", "Appendix", None, None, None])
    ai = len(out) - 1
    out[ai][2] = stub("Appendix", leaves("pages/05_appendix", "appendix"))
    return out


def link_map(rows):
    """basename -> (slug, title), plus the how-to readme special case."""
    m = {}
    for slug, title, _c, _p, src in rows:
        if src:
            m[src] = (slug, title)
    m["readme.md"] = ("how-to", "How-to guides")  # only ../03_how_to/readme.md is referenced
    return m


def rewrite(content, m):
    """Turn `path/to/NN_page.md` code spans into [Title](/beps/N/pages/slug)."""

    def sub(match):
        base = os.path.basename(match.group(1))
        if base in m:
            slug, title = m[base]
            return f"[{title}](/beps/{BEP}/pages/{slug})"
        return match.group(0)

    return re.sub(r"`((?:[\w.-]+/)*(?:\d+_[\w]+|readme)\.md)`", sub, content)


def main():
    note = sys.argv[1] if len(sys.argv) > 1 else "Republish from _planv2"
    mode = "current" if "--current" in sys.argv else "new"
    token = os.environ.get("BEPS_API_TOKEN")
    if not token:
        sys.exit("set BEPS_API_TOKEN (beps.boundaryml.com/profile)")
    rows = collect()
    m = link_map(rows)
    pages = []
    for slug, title, content, parent, _src in rows:
        p = {"slug": slug, "title": title, "content": rewrite(content, m)}
        if parent:
            p["parentSlug"] = parent
        pages.append(p)
    payload = {
        "number": BEP,
        "content": rewrite(read("readme.md"), m),
        "pages": pages,
        "editNote": note,
        "versionMode": mode,
    }
    req = urllib.request.Request(
        URL,
        data=json.dumps(payload).encode(),
        headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json"},
        method="PUT",
    )
    with urllib.request.urlopen(req) as resp:
        print(resp.read().decode())


if __name__ == "__main__":
    main()
