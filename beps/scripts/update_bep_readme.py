#!/usr/bin/env -S uv run --script
# /// script
# dependencies = []
# ///
import sys
import argparse
import re
import difflib
import subprocess
from pathlib import Path

from pathlib import Path
from datetime import datetime
import os

REPO_ROOT = Path(__file__).resolve().parents[2]
BEP_DIR = REPO_ROOT / "beps"
DOCS_DIR = BEP_DIR / "docs"
PROPOSALS_DIR = DOCS_DIR / "proposals"
README_PATH = DOCS_DIR / "README.md"

TABLE_START = "<!-- BEP-TABLE-START -->"
TABLE_END = "<!-- BEP-TABLE-END -->"

# Cache for canary ref detection
_CANARY_REF = None

def get_canary_ref() -> str:
    """Get the canary branch reference, trying local first then remote."""
    global _CANARY_REF
    if _CANARY_REF is not None:
        return _CANARY_REF
    
    # Try local 'canary' branch first
    result = subprocess.run(
        ["git", "rev-parse", "--verify", "canary"],
        cwd=REPO_ROOT,
        capture_output=True,
        check=False
    )
    if result.returncode == 0:
        _CANARY_REF = "canary"
        return _CANARY_REF
    
    # Try 'origin/canary' (common in CI/CD)
    result = subprocess.run(
        ["git", "rev-parse", "--verify", "origin/canary"],
        cwd=REPO_ROOT,
        capture_output=True,
        check=False
    )
    if result.returncode == 0:
        _CANARY_REF = "origin/canary"
        return _CANARY_REF
    
    # Fallback - return canary and let it fail gracefully
    _CANARY_REF = "canary"
    return _CANARY_REF


def parse_bep_file(path: Path):
    text = path.read_text(encoding="utf-8")
    
    # Parse metadata from frontmatter (simple key: value parsing)
    metadata = {}
    if text.startswith("---"):
        try:
            frontmatter_end = text.find("---", 3)
            if frontmatter_end != -1:
                frontmatter = text[3:frontmatter_end]
                for line in frontmatter.splitlines():
                    if ":" in line:
                        key, value = line.split(":", 1)
                        metadata[key.strip()] = value.strip()
                
                # Helper to safely get metadata
                def get_meta(key, default=None):
                    return metadata.get(key, default)
                
                # Parse ID from filename if not in metadata, or trust filename
                # Handle both BEP-XXX.md and BEP-XXX/README.md
                if path.name == "README.md":
                     m_filename = re.match(r"(BEP-\d+)", path.parent.name)
                else:
                     m_filename = re.match(r"(BEP-\d+)", path.name)
                
                bep_id = m_filename.group(1) if m_filename else "BEP-XXX"
                
                title = get_meta("title", "Untitled")
                status = get_meta("status", "Unknown")
                shepherds = get_meta("shepherds", "TBD")
                created = get_meta("created", "TBD")

                # Validate Shepherds
                if shepherds == "TBD" or not shepherds:
                    # If it's a draft, TBD is fine, otherwise warn/error
                    pass # Actually, let's just ensure it exists for now

                # Validate dates
                if created != "TBD":
                    try:
                        datetime.strptime(created, "%Y-%m-%d")
                    except ValueError:
                         raise ValueError(f"Invalid created date '{created}' in {path}. Must be YYYY-MM-DD.")

                # Check if file exists in canary branch
                try:
                    # Get path relative to repo root for git command
                    rel_path = path.relative_to(REPO_ROOT)
                    canary_ref = get_canary_ref()
                    file_in_canary = subprocess.run(
                        ["git", "cat-file", "-e", f"{canary_ref}:{rel_path}"],
                        cwd=REPO_ROOT,
                        capture_output=True,
                        check=False
                    ).returncode == 0
                except (subprocess.CalledProcessError, FileNotFoundError, ValueError):
                    file_in_canary = False

                # If file doesn't exist in canary, both dates should be TBD
                if not file_in_canary:
                    created = "TBD"
                    last_modified = "TBD"
                else:
                    # Get last modified time from git relative to canary branch
                    try:
                        # Get the last commit date for the file between canary and HEAD
                        canary_ref = get_canary_ref()
                        git_date = subprocess.check_output(
                            ["git", "log", f"{canary_ref}..HEAD", "-1", "--format=%cd", "--date=format:%Y-%m-%d", "--", str(path)],
                            text=True,
                            stderr=subprocess.DEVNULL
                        ).strip()
                        if git_date:
                            last_modified = git_date
                        else:
                            # File has no changes relative to canary
                            last_modified = "TBD"
                    except (subprocess.CalledProcessError, FileNotFoundError):
                         # Fallback to TBD if git fails
                        last_modified = "TBD"
                
                # Summary extraction (still heuristic based on markdown structure, or could be metadata)
                # Let's keep summary as first section after frontmatter for richness, 
                # or look for a summary metadata field.
                # For now, let's stick to body parsing for summary as it's usually longer.
                content_start = frontmatter_end + 3
                body = text[content_start:]
                
                summary = ""
                m_summary = re.search(r"^##\s+Summary\s*$", body, re.MULTILINE)
                if m_summary:
                    start = m_summary.end()
                    m_next = re.search(r"^##\s+", body[start:], re.MULTILINE)
                    if m_next:
                        end = start + m_next.start()
                        summary_block = body[start:end].strip()
                    else:
                        summary_block = body[start:].strip()
                    
                    summary = summary_block.split("\n\n", 1)[0].strip()
                    summary = " ".join(line.strip() for line in summary.splitlines())
                
                # Calculate relative link path
                # For directory-based BEPs (BEP-XXX/README.md), use just the directory name
                # MkDocs automatically resolves README.md from directories
                if path.name == "README.md":
                    link_path = f"{path.parent.name}/"
                else:
                    link_path = path.name

                return {
                    "id": bep_id,
                    "title": title,
                    "status": status,
                    "shepherds": shepherds,
                    "created": created,
                    "last_modified": last_modified,
                    "summary": summary,
                    "path": link_path,
                }
        except Exception as e:
            print(f"Error parsing frontmatter in {path}: {e}")
            return None
        except Exception as e:
            print(f"Error parsing frontmatter in {path}: {e}")
            return None

    # Fallback to old parsing if no frontmatter (for backward compat or mixed state)
    # 1. ID + title from "# BEP-XXX: Title"
    m_heading = re.search(r"^#\s+(BEP-\d+):\s*(.+)$", text, re.MULTILINE)
    if not m_heading:
        return None

    bep_id = m_heading.group(1).strip()
    title = m_heading.group(2).strip()

    # 2. Status from "**Status:** Foo"
    m_status = re.search(r"^\*\*Status:\*\*\s*(.+?)\s*$", text, re.MULTILINE)
    status = m_status.group(1).strip() if m_status else "Unknown"

    # 3. Summary: content under "## Summary" until next "## " or EOF
    summary = ""
    m_summary = re.search(r"^##\s+Summary\s*$", text, re.MULTILINE)
    if m_summary:
        start = m_summary.end()
        # Find next "## " after summary
        m_next = re.search(r"^##\s+", text[start:], re.MULTILINE)
        if m_next:
            end = start + m_next.start()
            summary_block = text[start:end].strip()
        else:
            summary_block = text[start:].strip()

        # Take just the first paragraph as the short description
        summary = summary_block.split("\n\n", 1)[0].strip()
        # Flatten newlines in the first paragraph
        summary = " ".join(line.strip() for line in summary.splitlines())

    return {
        "id": bep_id,
        "title": title,
        "status": status,
        "shepherds": "TBD",
        "summary": summary,
        "path": path.name,
    }


def get_status_badge(status: str) -> str:
    status_map = {
        "Draft": "https://img.shields.io/badge/Status-Draft-lightgrey",
        "Proposed": "https://img.shields.io/badge/Status-Proposed-yellow",
        "Accepted": "https://img.shields.io/badge/Status-Accepted-brightgreen",
        "Rejected": "https://img.shields.io/badge/Status-Rejected-red",
        "Superseded": "https://img.shields.io/badge/Status-Superseded-orange",
        "Implemented": "https://img.shields.io/badge/Status-Implemented-blue",
    }
    if status not in status_map:
        raise ValueError(f"Invalid status '{status}' found. Must be one of: {', '.join(status_map.keys())}")
    badge_url = status_map.get(status, f"https://img.shields.io/badge/Status-{status}-lightgrey")
    return f'<img src="{badge_url}" alt="{status}">'


def generate_table(entries):
    if not entries:
        return "_No BEPs found._"

    # Sort by numeric part of BEP-XXX
    def sort_key(e):
        m = re.match(r"BEP-(\d+)", e["id"])
        return int(m.group(1)) if m else 9999

    entries = sorted(entries, key=sort_key)

    lines = []
    lines.append("| Status | Meaning |")
    lines.append("| :--- | :--- |")
    lines.append(f"| {get_status_badge('Draft')} | Work in progress, not ready for review |")
    lines.append(f"| {get_status_badge('Proposed')} | Ready for review and discussion |")
    lines.append(f"| {get_status_badge('Accepted')} | Approved for implementation |")
    lines.append(f"| {get_status_badge('Implemented')} | Feature is live in BAML |")
    lines.append(f"| {get_status_badge('Rejected')} | Decided against |")
    lines.append(f"| {get_status_badge('Superseded')} | Replaced by another BEP |")
    lines.append("")
    
    lines.append("<table>")
    lines.append("  <thead>")
    lines.append("    <tr>")
    lines.append("      <th>BEP</th>")
    lines.append("    </tr>")
    lines.append("  </thead>")
    lines.append("  <tbody>")
    
    for e in entries:
        # Remove quotes from title if present
        title = e["title"].strip('"')
        link = f'<a href="./{e["path"]}"><strong>{e["id"]}</strong>: {title}</a>'
        desc = e["summary"] or ""
        status = e["status"]
        shepherds = e.get("shepherds", "TBD")
        
        if shepherds == "TBD" and status not in ["Superseded", "Rejected"]:
             raise ValueError(f"BEP {e['id']} ({status}) must have a shepherd assigned (currently 'TBD').")
        
        # Warn if shepherd format doesn't look like "Name <email>" but allow it for flexibility
        if status not in ["Superseded", "Rejected"] and "<" not in shepherds:
             # Optional: print(f"Warning: BEP {e['id']} shepherd '{shepherds}' might be missing email <...>.")
             pass

        status_badge = get_status_badge(status)
        
        # Handle multiple shepherds by splitting on comma
        shepherd_list = [s.strip() for s in shepherds.split(",")]
        shepherd_display = ", ".join(shepherd_list)
        
        lines.append("    <tr>")
        lines.append(f"      <td>{link} &nbsp; {status_badge}<br><br>{desc}<br><br><span style='font-size:0.8em; color:gray'>Shepherd(s): {shepherd_display}</span></td>")
        lines.append("    </tr>")

    lines.append("  </tbody>")
    lines.append("</table>")

    return "\n".join(lines)


def update_readme(table_md: str, check_only: bool = False):
    readme = README_PATH.read_text(encoding="utf-8")

    pattern = re.compile(
        rf"({re.escape(TABLE_START)})(.*)({re.escape(TABLE_END)})",
        re.DOTALL,
    )

    replacement = f"{TABLE_START}\n\n{table_md}\n\n{TABLE_END}"

    if pattern.search(readme):
        new_readme = pattern.sub(replacement, readme)
    else:
        # If markers are missing, append at the end
        new_readme = readme.rstrip() + "\n\n" + replacement + "\n"

    if check_only:
        if readme != new_readme:
            print(f"Error: {README_PATH} is out of sync.")
            
            print("\nDiff:")
            diff = difflib.unified_diff(
                readme.splitlines(),
                new_readme.splitlines(),
                fromfile=str(README_PATH),
                tofile="generated_readme",
                lineterm=""
            )
            for line in diff:
                print(line)

            print("\nTo fix this, run the following command locally and commit the changes:")
            print("\n    mise run bep:readme\n")
            sys.exit(1)
        else:
            print(f"Success: {README_PATH} is up to date.")
            sys.exit(0)

    README_PATH.write_text(new_readme, encoding="utf-8")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="Check if README is up-to-date without modifying it")
    args = parser.parse_args()

    # Support both flat files and directory-based BEPs in proposals dir
    bep_files = []
    if PROPOSALS_DIR.exists():
        bep_files.extend(sorted(list(PROPOSALS_DIR.glob("BEP-*.md")) + list(PROPOSALS_DIR.glob("BEP-*/README.md"))))
    
    # Legacy support for root level
    bep_files.extend(sorted(list(BEP_DIR.glob("BEP-*.md")) + list(BEP_DIR.glob("BEP-*/README.md"))))
    bep_files.extend(sorted(list(DOCS_DIR.glob("BEP-*.md")) + list(DOCS_DIR.glob("BEP-*/README.md"))))
    
    entries = []
    for f in bep_files:
        parsed = parse_bep_file(f)
        if parsed:
            # Adjust path for README linking
            if "proposals" in str(f.parent):
                 parsed["path"] = f"proposals/{parsed['path']}"
            entries.append(parsed)

    table_md = generate_table(entries)
    update_readme(table_md, check_only=args.check)
    print(f"Updated {README_PATH} with {len(entries)} BEP entries.")


if __name__ == "__main__":
    main()

