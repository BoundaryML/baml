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
            if f.is_file() and f.name != "bep":  # Don't count the script itself
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
    }

    return commands[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
`;
