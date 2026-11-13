/**
 * BEP CLI Script - embedded as a string constant.
 * The __BEP_API_BASE__ placeholder is replaced at export time.
 */
export const BEP_SCRIPT = `#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["httpx"]
# ///
"""
BEP CLI - Command-line tool for working with BAML Enhancement Proposals.

Usage:
    ./bep pull              # Fetch all BEPs from server into current directory
    ./bep list              # List local BEPs (--status STATUS, --json)
    ./bep new "Title"       # Scaffold a new BEP-?-slug/ locally
    ./bep diff <ref>        # Diff local vs server (--full for unified diff)
    ./bep push <ref>        # Dry-run diff + prompt to push (-y to confirm)
    ./bep open <number>     # Open BEP in browser
    ./bep bcs pull          # Team only: BCS design proposals into BCS/ (needs BCS_TOKEN)
    ./bep bcs new "T" f.html  # Team only: a new BCS proposal from an HTML page
    ./bep bcs push <number> # Team only: a new version of a BCS proposal
"""

import argparse
import json
import os
import re
import subprocess
import sys
import webbrowser
import zipfile
from datetime import date
from io import BytesIO
from pathlib import Path

# ─────────────────────────────────────────────────────────────────────────────
# Configuration
# ─────────────────────────────────────────────────────────────────────────────

SCRIPT_DIR = Path(__file__).resolve().parent
ALL_BEPS_DIR = SCRIPT_DIR  # BEPs are in the same directory as this script

# API endpoints - __BEP_API_BASE__ is replaced at export time
DEFAULT_API_BASE = "__BEP_API_BASE__"
API_BASE = os.environ.get("BEP_API_BASE", DEFAULT_API_BASE)
PULL_ENDPOINT = f"{API_BASE}/api/agent/beps/pull"
PUSH_ENDPOINT = f"{API_BASE}/api/agent/beps"

# Token for push operations (read from env)
API_TOKEN = os.environ.get("BEP_API_TOKEN", "")


# ─────────────────────────────────────────────────────────────────────────────
# Helpers
# ─────────────────────────────────────────────────────────────────────────────

def format_bep_number(num: int) -> str:
    return f"BEP-{num:03d}"


def parse_bep_ref(ref: str) -> tuple[int | None, str | None]:
    """
    Parse a BEP reference: number (41) or slug (my-feature for BEP-?-my-feature).
    Returns (number, slug) where one is None.
    """
    ref = ref.strip()

    # Try to parse as number
    if ref.isdigit():
        return int(ref), None

    # Try BEP-XXX format
    m = re.match(r"(?:BEP-)?(\\d+)", ref, re.IGNORECASE)
    if m:
        return int(m.group(1)), None

    # It's a slug
    return None, ref


def find_local_bep(ref: str) -> Path | None:
    """Find a local BEP folder by number or slug."""
    num, slug = parse_bep_ref(ref)

    if not ALL_BEPS_DIR.exists():
        return None

    for folder in ALL_BEPS_DIR.iterdir():
        if not folder.is_dir() or not folder.name.startswith("BEP-"):
            continue

        # Parse folder name: BEP-XXX-slug
        m = re.match(r"BEP-(\\d+|\\?)(?:-(.+))?", folder.name)
        if not m:
            continue

        folder_num_str = m.group(1)
        folder_num = int(folder_num_str) if folder_num_str != "?" else None
        folder_slug = m.group(2) or ""

        if num is not None and folder_num == num:
            return folder
        if slug is not None and slug.lower() == folder_slug.lower():
            return folder

    return None


def get_git_author() -> str:
    """Get git author name and email."""
    try:
        name = subprocess.check_output(
            ["git", "config", "user.name"], text=True
        ).strip()
        email = subprocess.check_output(
            ["git", "config", "user.email"], text=True
        ).strip()
        if name and email:
            return f"{name} <{email}>"
        return name or "<Author>"
    except Exception:
        return "<Author>"


def slugify(text: str) -> str:
    """Convert text to URL-safe slug."""
    text = text.lower()
    text = re.sub(r"[^a-z0-9]+", "-", text)
    return text.strip("-")[:50]


def read_local_bep(folder: Path) -> dict:
    """Read a local BEP folder and return its content."""
    result = {
        "folder": folder.name,
        "readme": "",
        "pages": [],
        "meta": None,
    }

    readme_path = folder / "README.md"
    if readme_path.exists():
        result["readme"] = readme_path.read_text(encoding="utf-8")

    meta_path = folder / "meta.json"
    if meta_path.exists():
        result["meta"] = json.loads(meta_path.read_text(encoding="utf-8"))

    pages_dir = folder / "pages"
    if pages_dir.exists() and pages_dir.is_dir():
        for page_file in sorted(pages_dir.rglob("*.md")):
            rel = page_file.relative_to(pages_dir)
            page = {
                "slug": page_file.stem,
                "content": page_file.read_text(encoding="utf-8"),
            }
            # Nested pages: the immediate folder name is the parent page's slug
            if len(rel.parts) > 1:
                page["parent_slug"] = rel.parts[-2]
            result["pages"].append(page)

    return result


# ─────────────────────────────────────────────────────────────────────────────
# Commands
# ─────────────────────────────────────────────────────────────────────────────

def cmd_pull(args: argparse.Namespace) -> int:
    """Fetch all BEPs from server and extract to current directory."""
    import httpx

    print(f"Pulling BEPs from {PULL_ENDPOINT}...")

    try:
        response = httpx.get(PULL_ENDPOINT, timeout=60.0)
        response.raise_for_status()
    except httpx.HTTPError as e:
        print(f"Error: Failed to fetch BEPs: {e}", file=sys.stderr)
        return 1

    # Extract ZIP
    zip_data = BytesIO(response.content)

    with zipfile.ZipFile(zip_data, "r") as zf:
        # First, show what would change
        existing_files = set()
        for f in ALL_BEPS_DIR.rglob("*"):
            # Don't count the script itself, or the BCS proposals "bep bcs pull" keeps here
            if f.is_file() and f.name != "bep" and f.relative_to(ALL_BEPS_DIR).parts[0] != "BCS":
                existing_files.add(f.relative_to(ALL_BEPS_DIR))

        new_files = set()
        for name in zf.namelist():
            # Strip "all-beps/" prefix if present
            if name.startswith("all-beps/"):
                rel_path = name[len("all-beps/"):]
            else:
                rel_path = name
            if rel_path and not rel_path.endswith("/"):
                new_files.add(Path(rel_path))

        added = new_files - existing_files
        removed = existing_files - new_files
        updated = new_files & existing_files

        if added or removed:
            print(f"\\nChanges:")
            if added:
                print(f"  + {len(added)} new files")
                for f in sorted(added)[:5]:
                    print(f"    + {f}")
                if len(added) > 5:
                    print(f"    ... and {len(added) - 5} more")
            if removed:
                print(f"  - {len(removed)} files to remove")
                for f in sorted(removed)[:5]:
                    print(f"    - {f}")
                if len(removed) > 5:
                    print(f"    ... and {len(removed) - 5} more")
            if updated:
                print(f"  ~ {len(updated)} files to update")
        else:
            print("\\nNo changes detected.")
            if not args.force:
                return 0

        if not args.yes:
            response_input = input("\\nApply changes? [y/N]: ").strip().lower()
            if response_input not in ("y", "yes"):
                print("Aborted.")
                return 0

        # Clear existing BEP folders and extract
        import shutil
        for item in ALL_BEPS_DIR.iterdir():
            # Keep the bep script and skills folder
            if item.name in ("bep", "skills"):
                continue
            if item.is_dir() and item.name.startswith("BEP-"):
                shutil.rmtree(item)
            elif item.is_dir() and item.name == "NEW-BEP":
                shutil.rmtree(item)
            elif item.name in ("Claude.md",):
                item.unlink()

        # Extract
        for name in zf.namelist():
            if name.startswith("all-beps/"):
                rel_path = name[len("all-beps/"):]
            else:
                rel_path = name

            if not rel_path or rel_path.endswith("/"):
                continue

            # Skip the bep script from the archive (we already have it)
            if rel_path == "bep":
                continue

            # Validate path to prevent directory traversal attacks
            target_path = (ALL_BEPS_DIR / rel_path).resolve()
            if not target_path.is_relative_to(ALL_BEPS_DIR.resolve()):
                print(f"Error: Unsafe path in archive: {rel_path}", file=sys.stderr)
                return 1

            target_path.parent.mkdir(parents=True, exist_ok=True)
            target_path.write_bytes(zf.read(name))

        print(f"\\nExtracted to {ALL_BEPS_DIR}")
        return 0


def cmd_list(args: argparse.Namespace) -> int:
    """List local BEPs."""
    beps = []
    for folder in sorted(ALL_BEPS_DIR.iterdir()):
        if not folder.is_dir() or not folder.name.startswith("BEP-"):
            continue

        meta_path = folder / "meta.json"
        if not meta_path.exists():
            continue

        try:
            meta = json.loads(meta_path.read_text(encoding="utf-8"))
            beps.append({
                "folder": folder.name,
                "number": meta.get("number"),
                "title": meta.get("title"),
                "status": meta.get("status", "unknown").lower(),
                "version": meta.get("version"),
                "isGoodReference": meta.get("isGoodReference", False),
            })
        except Exception:
            continue

    # Filter by status if requested
    if args.status:
        status_filter = args.status.lower()
        beps = [b for b in beps if b["status"] == status_filter]

    if args.json:
        print(json.dumps(beps, indent=2))
    else:
        if not beps:
            print("No BEPs found. Run './bep pull' first.")
            return 0

        print(f"{'ID':<12} {'Status':<12} {'Title'}")
        print("-" * 60)
        for b in beps:
            star = "⭐ " if b.get("isGoodReference") else ""
            num = b['number']
            if num is not None:
                print(f"BEP-{num:03d}     {b['status']:<12} {star}{b['title']}")
            else:
                print(f"BEP-???     {b['status']:<12} {star}{b['title']}")

    return 0


def cmd_new(args: argparse.Namespace) -> int:
    """Scaffold a new BEP locally."""
    title = args.title
    author = args.author or get_git_author()

    # Create with unknown number (BEP-?-slug)
    slug = slugify(title)
    folder_name = f"BEP-?-{slug}"
    folder_path = ALL_BEPS_DIR / folder_name

    if folder_path.exists():
        print(f"Error: {folder_path} already exists.", file=sys.stderr)
        return 1

    folder_path.mkdir(parents=True)

    # Create meta.json (partial, will be filled on push)
    meta = {
        "id": "BEP-???",
        "number": None,
        "title": title,
        "status": "draft",
        "version": 1,
        "shepherds": [author.split(" <")[0] if " <" in author else author],
        "created": date.today().isoformat(),
        "updated": date.today().isoformat(),
        "isGoodReference": False,
        "openIssueCount": 0,
        "pages": [],
    }
    (folder_path / "meta.json").write_text(json.dumps(meta, indent=2), encoding="utf-8")

    # Create README.md with template
    readme_content = f"""# {title}

## Summary

A concise explanation (3–8 sentences) of what this proposal does and what it enables for BAML users.

\\\`\\\`\\\`baml
// Example code showing the feature
\\\`\\\`\\\`

## Prior Art

Research, other languages, related work.

## Proposed Design

The detailed technical design. This is the heart of the BEP.

### Syntax

\\\`\\\`\\\`baml
// Example BAML syntax
\\\`\\\`\\\`

### Semantics

How the compiler/runtime handles this.

## Design Tradeoffs

What alternatives were considered? Why this approach?

## Open Questions

Unresolved decisions, future work.
"""

    (folder_path / "README.md").write_text(readme_content, encoding="utf-8")

    # Create empty pages directory
    (folder_path / "pages").mkdir()

    print(f"Created {folder_path}")
    print(f"\\nNext steps:")
    print(f"  1. Edit {folder_path}/README.md")
    print(f"  2. Run: ./bep diff {slug}")
    print(f"  3. Run: ./bep push {slug}")

    return 0


def cmd_diff(args: argparse.Namespace) -> int:
    """Show diff between local and server."""
    import httpx

    ref = args.ref
    local_folder = find_local_bep(ref)

    if not local_folder:
        print(f"Error: Could not find local BEP matching '{ref}'.", file=sys.stderr)
        print("Available BEPs:")
        cmd_list(argparse.Namespace(status=None, json=False))
        return 1

    local_bep = read_local_bep(local_folder)
    meta = local_bep.get("meta") or {}
    bep_number = meta.get("number")

    # For new BEPs (BEP-?-*), there's nothing to diff against
    if bep_number is None:
        print(f"New BEP: {local_folder.name}")
        print("\\nContent to be created:")
        print("-" * 40)
        if args.full:
            print(local_bep["readme"])
        else:
            lines = local_bep["readme"].split("\\n")[:20]
            print("\\n".join(lines))
            if len(local_bep["readme"].split("\\n")) > 20:
                print(f"... ({len(local_bep['readme'].split(chr(10)))} total lines)")

        if local_bep["pages"]:
            print(f"\\nPages: {len(local_bep['pages'])}")
            for page in local_bep["pages"]:
                print(f"  - {page['slug']}.md")

        return 0

    # Fetch current server version
    print(f"Fetching {format_bep_number(bep_number)} from server...")

    try:
        response = httpx.get(
            PUSH_ENDPOINT,
            params={"name": str(bep_number), "omitOtherVersions": "true"},
            timeout=30.0,
        )
        response.raise_for_status()
        server_data = response.json()
    except httpx.HTTPError as e:
        print(f"Error: Failed to fetch from server: {e}", file=sys.stderr)
        return 1

    # Extract server content
    server_files = {f["path"]: f["content"] for f in server_data.get("files", [])}
    server_readme = server_files.get("README.md", "")

    # Compare
    import difflib

    local_readme = local_bep["readme"]

    if args.full:
        diff = difflib.unified_diff(
            server_readme.splitlines(keepends=True),
            local_readme.splitlines(keepends=True),
            fromfile=f"server/{format_bep_number(bep_number)}/README.md",
            tofile=f"local/{local_folder.name}/README.md",
        )
        diff_text = "".join(diff)
        if diff_text:
            print(diff_text)
        else:
            print("No differences in README.md")
    else:
        # Summary diff
        server_lines = len(server_readme.splitlines())
        local_lines = len(local_readme.splitlines())

        if server_readme == local_readme:
            print("README.md: No changes")
        else:
            print(f"README.md: {server_lines} → {local_lines} lines")

    # Compare pages (tracking each page's immediate parent to detect moves)
    server_pages = {}
    for f_path, content in server_files.items():
        if f_path.startswith("pages/") and f_path.endswith(".md"):
            # Slug is the basename; nested pages live at pages/<ancestors>/<slug>.md
            parts = f_path[:-3].split("/")[1:]
            slug = parts[-1]
            parent = parts[-2] if len(parts) > 1 else None
            server_pages[slug] = (parent, content)

    local_pages = {
        p["slug"]: (p.get("parent_slug"), p["content"]) for p in local_bep["pages"]
    }

    all_slugs = set(server_pages.keys()) | set(local_pages.keys())
    for slug in sorted(all_slugs):
        server_parent, server_content = server_pages.get(slug, (None, ""))
        local_parent, local_content = local_pages.get(slug, (None, ""))

        if slug not in server_pages:
            print(f"pages/{slug}.md: NEW ({len(local_content.splitlines())} lines)")
        elif slug not in local_pages:
            print(f"pages/{slug}.md: DELETED")
        else:
            if server_parent != local_parent:
                print(
                    f"pages/{slug}.md: MOVED "
                    f"({server_parent or '(top level)'} -> {local_parent or '(top level)'})"
                )
            if server_content != local_content:
                print(f"pages/{slug}.md: MODIFIED")
                if args.full:
                    diff = difflib.unified_diff(
                        server_content.splitlines(keepends=True),
                        local_content.splitlines(keepends=True),
                        fromfile=f"server/pages/{slug}.md",
                        tofile=f"local/pages/{slug}.md",
                    )
                    print("".join(diff))

    return 0


def cmd_push(args: argparse.Namespace) -> int:
    """Push local BEP to server."""
    import httpx

    if not API_TOKEN:
        print("Error: BEP_API_TOKEN environment variable not set.", file=sys.stderr)
        print(f"Get your token from: {API_BASE}/profile", file=sys.stderr)
        return 1

    ref = args.ref
    local_folder = find_local_bep(ref)

    if not local_folder:
        print(f"Error: Could not find local BEP matching '{ref}'.", file=sys.stderr)
        return 1

    local_bep = read_local_bep(local_folder)
    meta = local_bep.get("meta") or {}
    bep_number = meta.get("number")

    # Prepare payload
    readme_content = local_bep["readme"]
    pages = []
    for page in local_bep["pages"]:
        # Extract title from first heading or use slug
        title_match = re.search(r"^#\\s+(.+)$", page["content"], re.MULTILINE)
        title = title_match.group(1).strip() if title_match else page["slug"].title()
        page_payload = {
            "slug": page["slug"],
            "title": title,
            "content": page["content"],
        }
        if page.get("parent_slug"):
            page_payload["parentSlug"] = page["parent_slug"]
        pages.append(page_payload)

    # Show diff first
    print("Changes to push:")
    print("-" * 40)

    if bep_number is None:
        # New BEP
        print(f"CREATE new BEP: {meta.get('title', 'Untitled')}")
        print(f"  README.md: {len(readme_content.splitlines())} lines")
        if pages:
            print(f"  Pages: {len(pages)}")
            for p in pages:
                print(f"    - {p['slug']}.md ({len(p['content'].splitlines())} lines)")
    else:
        # Update existing
        print(f"UPDATE {format_bep_number(bep_number)}: {meta.get('title', '')}")
        # Run diff to show changes
        cmd_diff(argparse.Namespace(ref=ref, full=False))

    print()

    if not args.yes:
        response_input = input("Push these changes? [y/N]: ").strip().lower()
        if response_input not in ("y", "yes"):
            print("Aborted.")
            return 0

    headers = {
        "Authorization": f"Bearer {API_TOKEN}",
        "Content-Type": "application/json",
    }

    if bep_number is None:
        # Create new BEP
        payload = {
            "title": meta.get("title", "Untitled"),
            "content": readme_content,
        }
        if pages:
            payload["pages"] = pages

        try:
            response = httpx.post(
                PUSH_ENDPOINT,
                headers=headers,
                json=payload,
                timeout=30.0,
            )
            response.raise_for_status()
            result = response.json()

            print(f"\\n✓ Created {result.get('formattedId', 'BEP')}")
            print(f"  URL: {result.get('url', '')}")

            # Update local meta.json with the new number
            new_number = result.get("number")
            if new_number:
                meta["number"] = new_number
                meta["id"] = format_bep_number(new_number)
                (local_folder / "meta.json").write_text(
                    json.dumps(meta, indent=2), encoding="utf-8"
                )

                # Rename folder
                new_folder_name = f"BEP-{new_number:03d}-{slugify(meta.get('title', ''))}"
                new_folder_path = ALL_BEPS_DIR / new_folder_name
                local_folder.rename(new_folder_path)
                print(f"  Renamed to: {new_folder_name}")

        except httpx.HTTPError as e:
            print(f"Error: Failed to create BEP: {e}", file=sys.stderr)
            if hasattr(e, "response") and e.response is not None:
                try:
                    print(f"  {e.response.json()}", file=sys.stderr)
                except Exception:
                    pass
            return 1
    else:
        # Update existing BEP
        payload = {
            "number": bep_number,
            "content": readme_content,
            "editNote": args.note or "Updated via CLI",
            "versionMode": "new" if args.new_version else "current",
            "pages": pages,  # Always send pages, even if empty (to allow clearing)
        }

        try:
            response = httpx.put(
                PUSH_ENDPOINT,
                headers=headers,
                json=payload,
                timeout=30.0,
            )
            response.raise_for_status()
            result = response.json()

            print(f"\\n✓ Updated {format_bep_number(bep_number)}")
            print(f"  Version: {result.get('versionNumber', '?')} ({result.get('versionAction', '')})")
            if result.get("pagesCreated"):
                print(f"  Pages created: {result['pagesCreated']}")
            if result.get("pagesUpdated"):
                print(f"  Pages updated: {result['pagesUpdated']}")
            if result.get("pagesDeleted"):
                print(f"  Pages deleted: {result['pagesDeleted']}")
            print(f"  URL: {result.get('url', '')}")

        except httpx.HTTPError as e:
            print(f"Error: Failed to update BEP: {e}", file=sys.stderr)
            if hasattr(e, "response") and e.response is not None:
                try:
                    print(f"  {e.response.json()}", file=sys.stderr)
                except Exception:
                    pass
            return 1

    return 0


def cmd_open(args: argparse.Namespace) -> int:
    """Open a BEP in the browser."""
    num, slug = parse_bep_ref(args.ref)

    if num is not None:
        url = f"{API_BASE}/beps/{num}"
    else:
        # Try to find the local BEP to get the number
        local_folder = find_local_bep(args.ref)
        if local_folder:
            meta_path = local_folder / "meta.json"
            if meta_path.exists():
                meta = json.loads(meta_path.read_text(encoding="utf-8"))
                num = meta.get("number")
                if num:
                    url = f"{API_BASE}/beps/{num}"
                else:
                    print(f"Error: BEP has no number assigned yet.", file=sys.stderr)
                    return 1
            else:
                print(f"Error: No meta.json found for {args.ref}.", file=sys.stderr)
                return 1
        else:
            print(f"Error: Could not find BEP matching '{args.ref}'.", file=sys.stderr)
            return 1

    print(f"Opening {url}")
    webbrowser.open(url)
    return 0


# ─────────────────────────────────────────────────────────────────────────────
# Main
# ─────────────────────────────────────────────────────────────────────────────

def bep_number_of(ref: str) -> int | None:
    """A BEP's number from 41, BEP-041 or a local folder's slug."""
    number, _ = parse_bep_ref(ref)
    if number is not None:
        return number
    folder = find_local_bep(ref)
    if folder and (folder / "meta.json").exists():
        return json.loads((folder / "meta.json").read_text(encoding="utf-8")).get("number")
    return None


def cmd_comments(args: argparse.Namespace) -> int:
    import httpx

    number = bep_number_of(args.ref)
    if number is None:
        print(f"Error: no BEP {args.ref}", file=sys.stderr)
        return 1
    output = "json" if args.json else (args.format or "markdown")
    try:
        response = httpx.get(
            f"{API_BASE}/api/agent/beps/{number}/comments",
            params={"includeResolved": "true" if args.include_resolved else "false", "format": output},
            timeout=60.0,
        )
        response.raise_for_status()
    except httpx.HTTPError as e:
        print(f"Error: failed to fetch comments: {e}", file=sys.stderr)
        return 1
    if output == "json":
        print(json.dumps(response.json(), indent=2))
    else:
        print(response.text)
    return 0


def cmd_reply(args: argparse.Namespace) -> int:
    import httpx

    if not API_TOKEN:
        print("Error: BEP_API_TOKEN environment variable not set.", file=sys.stderr)
        return 1
    number = bep_number_of(args.ref)
    if number is None:
        print(f"Error: no BEP {args.ref}", file=sys.stderr)
        return 1
    try:
        response = httpx.post(
            f"{API_BASE}/api/agent/beps/{number}/comments/reply",
            headers={"Authorization": f"Bearer {API_TOKEN}"},
            json={"commentId": args.comment_id, "content": args.content, "type": args.type},
            timeout=60.0,
        )
    except httpx.HTTPError as e:
        print(f"Error: failed to reply: {e}", file=sys.stderr)
        return 1
    if response.status_code >= 400:
        print(f"Error ({response.status_code}): {response.text}", file=sys.stderr)
        return 1
    print(f"Replied on {format_bep_number(number)}")
    return 0


# ─────────────────────────────────────────────────────────────────────────────
# BCS design proposals (team only)
# ─────────────────────────────────────────────────────────────────────────────
# System design proposals for Boundary Cloud, each one HTML page. They come down only through
# "bep bcs pull", never in the export, and every call needs BCS_TOKEN, which a team member gets
# at /bcs/token on the site.

BCS_DIR = SCRIPT_DIR / "BCS"
BCS_ENDPOINT = f"{API_BASE}/api/agent/bcs"


def format_bcs_number(num: int) -> str:
    return f"BCS-{num:03d}"


def bcs_call(method: str, path: str = "", body: dict | None = None) -> dict:
    """A call to the BCS routes with BCS_TOKEN; exits with the server's reason on failure."""
    import httpx

    token = os.environ.get("BCS_TOKEN", "")
    if not token:
        print(f"Error: set BCS_TOKEN. Get yours at {API_BASE}/bcs/token", file=sys.stderr)
        sys.exit(1)
    try:
        response = httpx.request(
            method,
            BCS_ENDPOINT + path,
            headers={"Authorization": f"Bearer {token}"},
            json=body,
            timeout=120.0,
        )
    except httpx.HTTPError as e:
        print(f"Error: could not reach {BCS_ENDPOINT}: {e}", file=sys.stderr)
        sys.exit(1)
    if response.status_code >= 400:
        try:
            reason = response.json().get("error", response.text)
        except ValueError:
            reason = response.text
        print(f"Error ({response.status_code}): {reason}", file=sys.stderr)
        sys.exit(1)
    return response.json()


def find_local_bcs(ref: str) -> Path | None:
    """The local folder of BCS proposal ref: 1, 001 or BCS-001."""
    m = re.match(r"(?:BCS-)?([0-9]+)$", ref.strip(), re.IGNORECASE)
    if not m or not BCS_DIR.exists():
        return None
    prefix = format_bcs_number(int(m.group(1)))
    for folder in sorted(BCS_DIR.iterdir()):
        if folder.is_dir() and (folder.name == prefix or folder.name.startswith(prefix + "-")):
            return folder
    return None


BCS_EXTENSIONS = {"markdown": ".md", "html": ".html"}


def bcs_format_of(path: Path) -> str | None:
    """A page file's format from its extension, or None for any other file."""
    suffix = path.suffix.lower()
    if suffix in (".md", ".markdown"):
        return "markdown"
    if suffix in (".html", ".htm"):
        return "html"
    return None


def bcs_page_path(page: dict, parents: dict) -> Path:
    """Where a page lives in a proposal folder: index.* for the main page, else pages/<ancestors>/<slug>.*"""
    ext = BCS_EXTENSIONS[page["format"]]
    if page["slug"] == "":
        return Path("index" + ext)
    chain = []
    up = page.get("parentSlug")
    while up:
        chain.insert(0, up)
        up = parents.get(up)
    return Path("pages", *chain, page["slug"] + ext)


def bcs_canonical(pages: list) -> list:
    """Pages as plain comparable values, so a digest or a compare ignores key order and missing parents."""
    return [
        {"slug": p["slug"], "parentSlug": p.get("parentSlug") or None, "format": p["format"], "content": p["content"]}
        for p in pages
    ]


def bcs_digest(pages: list) -> str:
    import hashlib

    return hashlib.sha256(json.dumps(bcs_canonical(pages), sort_keys=True).encode("utf-8")).hexdigest()


def read_local_bcs_pages(folder: Path) -> list:
    """A proposal folder's pages: index.md or index.html, then pages/ in the order last pulled, new ones last."""
    mains = [f for f in sorted(folder.glob("index.*")) if bcs_format_of(f)]
    if len(mains) != 1:
        raise ValueError(f"{folder} needs exactly one index.md or index.html")
    pages = [{"slug": "", "format": bcs_format_of(mains[0]), "content": mains[0].read_text(encoding="utf-8")}]
    found = []
    pages_dir = folder / "pages"
    if pages_dir.is_dir():
        for f in sorted(pages_dir.rglob("*")):
            fmt = bcs_format_of(f) if f.is_file() else None
            if not fmt:
                continue
            rel = f.relative_to(pages_dir)
            page = {"slug": f.stem, "format": fmt, "content": f.read_text(encoding="utf-8"), "_path": rel.with_suffix("").parts}
            # Nested pages: the immediate folder name is the parent page's slug.
            if len(rel.parts) > 1:
                page["parentSlug"] = rel.parts[-2]
            found.append(page)
    meta_path = folder / "meta.json"
    order = json.loads(meta_path.read_text(encoding="utf-8")).get("pages", []) if meta_path.exists() else []
    rank = {slug: i for i, slug in enumerate(order)}
    # Pages pulled before keep their order; new ones follow, each parent before what is inside it.
    found.sort(key=lambda page: (rank.get(page["slug"], len(rank)), page.pop("_path")))
    return pages + found


def read_bcs_source(path: Path) -> list:
    """Pages to create a proposal from: one .md or .html file, or a proposal folder."""
    if path.is_dir():
        return read_local_bcs_pages(path)
    fmt = bcs_format_of(path)
    if not fmt:
        raise ValueError(f"{path} is not a .md or .html file, nor a folder with index.md")
    return [{"slug": "", "format": fmt, "content": path.read_text(encoding="utf-8")}]


def bcs_page_files(folder: Path) -> list:
    """The page files in a proposal folder: index.* and pages/**, with a page extension. Other files are the user's."""
    files = [f for f in folder.glob("index.*") if f.is_file() and bcs_format_of(f)]
    pages_dir = folder / "pages"
    if pages_dir.is_dir():
        files += [f for f in pages_dir.rglob("*") if f.is_file() and bcs_format_of(f)]
    return files


def write_local_bcs(proposal: dict) -> Path:
    """Writes a proposal's pages and metadata, replacing only its page files, and renames its folder if its title changed."""
    number = proposal["number"]
    folder = BCS_DIR / f"{format_bcs_number(number)}-{slugify(proposal['title'])}"
    old = find_local_bcs(str(number))
    if old and old != folder:
        old.rename(folder)
    folder.mkdir(parents=True, exist_ok=True)
    for f in bcs_page_files(folder):
        f.unlink()
    pages_dir = folder / "pages"
    if pages_dir.is_dir():
        # Folders left empty by pages that moved or went away; anything else stays.
        for d in sorted((d for d in pages_dir.rglob("*") if d.is_dir()), key=lambda d: len(d.parts), reverse=True):
            if not any(d.iterdir()):
                d.rmdir()
    pages = proposal["pages"]
    parents = {p["slug"]: p.get("parentSlug") for p in pages}
    for page in pages:
        path = folder / bcs_page_path(page, parents)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(page["content"], encoding="utf-8")
    meta = {key: proposal[key] for key in ("number", "title", "status", "authors", "version") if key in proposal}
    meta["pages"] = [p["slug"] for p in pages if p["slug"]]
    # What the server had, so a later pull can tell a local edit from an old copy.
    meta["pulledSha256"] = bcs_digest(pages)
    (folder / "meta.json").write_text(json.dumps(meta, indent=2) + chr(10), encoding="utf-8")
    return folder


def bcs_local_edits(folder: Path) -> bool:
    """Whether the local pages may differ from what was last pulled or pushed; without a record of that, assume so."""
    import hashlib

    meta_path = folder / "meta.json"
    if not meta_path.exists():
        return True
    meta = json.loads(meta_path.read_text(encoding="utf-8"))
    pulled = meta.get("pulledSha256")
    if pulled is None:
        return True
    if "pages" not in meta:
        # Pulled by a bep script from before sub-pages: the digest is of index.html alone.
        page = folder / "index.html"
        return not page.exists() or hashlib.sha256(page.read_bytes()).hexdigest() != pulled
    try:
        return bcs_digest(read_local_bcs_pages(folder)) != pulled
    except ValueError:
        return True


def cmd_bcs_pull(args: argparse.Namespace) -> int:
    """Every proposal's current pages, one request each; local edits are kept unless --force."""
    listed = bcs_call("GET").get("proposals", [])
    kept = 0
    for summary in listed:
        number = summary["number"]
        local = find_local_bcs(str(number))
        if local and bcs_local_edits(local) and not args.force:
            print(f"  {local.name}  kept: it has edits you have not pushed (bep bcs push {number}, or pull --force)")
            kept += 1
            continue
        proposal = bcs_call("GET", f"/{number}")
        folder = write_local_bcs(proposal)
        count = len(proposal["pages"])
        print(f"  {folder.name}  v{proposal['version']}  {proposal['status']}  {count} page{'' if count == 1 else 's'}")
    print(f"Pulled {len(listed) - kept} BCS proposal(s) into {BCS_DIR}" + (f", kept {kept} with local edits" if kept else ""))
    return 0


def cmd_bcs_list(args: argparse.Namespace) -> int:
    if not BCS_DIR.exists():
        print("No BCS proposals here yet: run bep bcs pull")
        return 0
    for folder in sorted(BCS_DIR.iterdir()):
        meta_path = folder / "meta.json"
        if meta_path.exists():
            meta = json.loads(meta_path.read_text(encoding="utf-8"))
            print(f"{format_bcs_number(meta['number'])}  v{meta.get('version', '?')}  {meta.get('status', '?'):<12} {meta['title']}")
    return 0


def cmd_bcs_new(args: argparse.Namespace) -> int:
    path = Path(args.file)
    if not path.exists():
        print(f"Error: no file or folder {path}", file=sys.stderr)
        return 1
    try:
        pages = read_bcs_source(path)
    except ValueError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1
    created = bcs_call("POST", body={"title": args.title, "pages": pages, "status": args.status})
    number = created["number"]
    folder = write_local_bcs(bcs_call("GET", f"/{number}"))
    print(f"Created {format_bcs_number(number)} with {len(pages)} page(s): {API_BASE}/bcs/{number}")
    print(f"Local copy: {folder}")
    return 0


def bcs_describe_changes(remote: list, local: list) -> list:
    """One line per page that differs: added, removed, moved or changed (with line counts)."""
    import difflib

    before = {p["slug"]: p for p in bcs_canonical(remote)}
    after = {p["slug"]: p for p in bcs_canonical(local)}
    lines = []
    for slug, page in after.items():
        name = "main page" if slug == "" else slug
        was = before.get(slug)
        if not was:
            lines.append(f"  + {name} (new, {len(page['content'].splitlines())} lines)")
            continue
        notes = []
        if was["format"] != page["format"]:
            notes.append(f"now {page['format']}")
        if was["parentSlug"] != page["parentSlug"]:
            notes.append(f"moved under {page['parentSlug'] or 'the main page'}")
        if was["content"] != page["content"]:
            changed = [
                line for line in difflib.unified_diff(was["content"].splitlines(), page["content"].splitlines(), lineterm="")
                if line[:1] in "+-" and line[:3] not in ("+++", "---")
            ]
            added = sum(1 for line in changed if line.startswith("+"))
            notes.append(f"{added} line(s) added, {len(changed) - added} removed")
        if notes:
            lines.append(f"  ~ {name}: " + ", ".join(notes))
    for slug in before:
        if slug not in after:
            lines.append(f"  - {slug} (removed)")
    if not lines and [p["slug"] for p in remote] != [p["slug"] for p in local]:
        lines.append("  ~ page order")
    return lines


def cmd_bcs_push(args: argparse.Namespace) -> int:
    folder = find_local_bcs(args.ref)
    if not folder:
        print(f"Error: no local BCS proposal {args.ref}: run bep bcs pull", file=sys.stderr)
        return 1
    meta = json.loads((folder / "meta.json").read_text(encoding="utf-8"))
    try:
        local = read_local_bcs_pages(folder)
    except ValueError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1
    number = meta["number"]
    remote = bcs_call("GET", f"/{number}")
    if remote["version"] != meta.get("version") and not args.force:
        print(f"Error: the server has v{remote['version']}, you edited v{meta.get('version')}. Pull first, or --force.", file=sys.stderr)
        return 1
    changes = bcs_describe_changes(remote["pages"], local)
    if not changes:
        print(f"{format_bcs_number(number)} matches the server's v{remote['version']}: nothing to push")
        return 0
    print(f"{format_bcs_number(number)} against v{remote['version']}:")
    for line in changes:
        print(line)
    if not args.yes:
        answer = input(f"Push as v{remote['version'] + 1}? [y/N] ").strip().lower()
        if answer not in ("y", "yes"):
            print("Not pushed")
            return 0
    saved = bcs_call("PUT", f"/{number}", body={
        "pages": local,
        "note": args.note,
        "baseVersion": None if args.force else meta.get("version"),
    })
    meta["version"] = saved["version"]
    meta["pages"] = [p["slug"] for p in local if p["slug"]]
    meta["pulledSha256"] = bcs_digest(local)
    (folder / "meta.json").write_text(json.dumps(meta, indent=2) + chr(10), encoding="utf-8")
    print(f"Pushed {format_bcs_number(number)} v{saved['version']}: {API_BASE}/bcs/{number}")
    return 0


def cmd_bcs_open(args: argparse.Namespace) -> int:
    m = re.match(r"(?:BCS-)?([0-9]+)$", args.ref.strip(), re.IGNORECASE)
    if not m:
        print(f"Error: {args.ref} is not a BCS number", file=sys.stderr)
        return 1
    webbrowser.open(f"{API_BASE}/bcs/{int(m.group(1))}")
    return 0


def cmd_bcs(args: argparse.Namespace) -> int:
    commands = {
        "pull": cmd_bcs_pull,
        "list": cmd_bcs_list,
        "new": cmd_bcs_new,
        "push": cmd_bcs_push,
        "open": cmd_bcs_open,
    }
    if not getattr(args, "bcs_command", None):
        print("Usage: bep bcs {pull,list,new,push,open}")
        return 1
    return commands[args.bcs_command](args)


def main():
    parser = argparse.ArgumentParser(
        description="BEP CLI - Work with BAML Enhancement Proposals",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    subparsers = parser.add_subparsers(dest="command", help="Available commands")

    # pull
    pull_parser = subparsers.add_parser("pull", help="Fetch all BEPs from server")
    pull_parser.add_argument("-y", "--yes", action="store_true", help="Skip confirmation")
    pull_parser.add_argument("-f", "--force", action="store_true", help="Force even if no changes")

    # list
    list_parser = subparsers.add_parser("list", help="List local BEPs")
    list_parser.add_argument("--status", help="Filter by status (draft, proposed, accepted, etc.)")
    list_parser.add_argument("--json", action="store_true", help="Output as JSON")

    # new
    new_parser = subparsers.add_parser("new", help="Scaffold a new BEP")
    new_parser.add_argument("title", help="Title of the proposal")
    new_parser.add_argument("--author", help="Author name")

    # diff
    diff_parser = subparsers.add_parser("diff", help="Diff local vs server")
    diff_parser.add_argument("ref", help="BEP number (e.g., 41) or slug (e.g., my-feature)")
    diff_parser.add_argument("--full", action="store_true", help="Show full unified diff")

    # push
    push_parser = subparsers.add_parser("push", help="Push local BEP to server")
    push_parser.add_argument("ref", help="BEP number or slug")
    push_parser.add_argument("-y", "--yes", action="store_true", help="Skip confirmation")
    push_parser.add_argument("-n", "--note", help="Edit note for this version")
    push_parser.add_argument("--new-version", action="store_true", help="Create a new version")

    # open
    open_parser = subparsers.add_parser("open", help="Open BEP in browser")
    open_parser.add_argument("ref", help="BEP number or slug")

    # comments
    comments_parser = subparsers.add_parser("comments", help="Fetch comments for a BEP")
    comments_parser.add_argument("ref", help="BEP number or slug")
    comments_parser.add_argument("--include-resolved", action="store_true", help="Include resolved comments")
    comments_parser.add_argument("--format", choices=["json", "markdown"], default=None, help="Output format")
    comments_parser.add_argument("--json", action="store_true", help="Output as JSON")

    # reply
    reply_parser = subparsers.add_parser("reply", help="Reply to a comment on a BEP")
    reply_parser.add_argument("ref", help="BEP number or slug")
    reply_parser.add_argument("comment_id", help="ID of the comment to reply to")
    reply_parser.add_argument("content", help="Reply content")
    reply_parser.add_argument("--type", choices=["discussion", "concern", "question"], default="discussion", help="Comment type")

    # bcs: team-only design proposals
    bcs_parser = subparsers.add_parser("bcs", help="Team only: BCS design proposals (needs BCS_TOKEN)")
    bcs_sub = bcs_parser.add_subparsers(dest="bcs_command")
    bcs_pull = bcs_sub.add_parser("pull", help="Fetch every BCS proposal into BCS/ (index.md or index.html, and pages/)")
    bcs_pull.add_argument("-f", "--force", action="store_true", help="Overwrite local edits you have not pushed")
    bcs_sub.add_parser("list", help="List local BCS proposals")
    bcs_new = bcs_sub.add_parser("new", help="Create a BCS proposal from a page or a folder of pages")
    bcs_new.add_argument("title", help="Title of the proposal")
    bcs_new.add_argument("file", help="A .md or .html page, or a folder with index.md (or index.html) and pages/")
    bcs_new.add_argument("--status", default="draft", choices=["draft", "proposed", "pending", "accepted", "implemented", "rejected", "superseded"], help="Its status")
    bcs_push = bcs_sub.add_parser("push", help="Save a local BCS proposal as a new version")
    bcs_push.add_argument("ref", help="BCS number (e.g., 1 or BCS-001)")
    bcs_push.add_argument("-y", "--yes", action="store_true", help="Skip confirmation")
    bcs_push.add_argument("-n", "--note", help="What changed in this version")
    bcs_push.add_argument("-f", "--force", action="store_true", help="Push even if the server moved on")
    bcs_open = bcs_sub.add_parser("open", help="Open a BCS proposal in the browser")
    bcs_open.add_argument("ref", help="BCS number")

    args = parser.parse_args()

    if args.command is None:
        parser.print_help()
        return 0

    commands = {
        "pull": cmd_pull,
        "list": cmd_list,
        "new": cmd_new,
        "diff": cmd_diff,
        "push": cmd_push,
        "open": cmd_open,
        "comments": cmd_comments,
        "reply": cmd_reply,
        "bcs": cmd_bcs,
    }

    return commands[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
`;
