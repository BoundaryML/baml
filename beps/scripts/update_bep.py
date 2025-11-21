#!/usr/bin/env -S uv run --script
# /// script
# dependencies = []
# ///
import argparse
import sys
import re
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
BEP_DIR = REPO_ROOT / "beps"
DOCS_DIR = BEP_DIR / "docs"
PROPOSALS_DIR = DOCS_DIR / "proposals"

VALID_STATUSES = [
    "Draft",
    "Proposed",
    "Accepted",
    "Rejected",
    "Superseded",
    "Implemented",
]

def find_bep_path(bep_identifier):
    # Try finding by directory name match first in proposals dir
    # Could be BEP-001, BEP-001-exceptions, etc.
    
    # Case 1: Exact folder match
    exact = PROPOSALS_DIR / bep_identifier
    if exact.is_dir() and (exact / "README.md").exists():
        return exact / "README.md"
        
    # Case 1b: Exact match ignoring path prefixes if passed via CLI completion
    clean_id = Path(bep_identifier).name
    exact_clean = PROPOSALS_DIR / clean_id
    if exact_clean.is_dir() and (exact_clean / "README.md").exists():
            return exact_clean / "README.md"
    
    # Case 2: Partial match (e.g. "001")
    # Normalize to integer if possible
    search_num = None
    m = re.match(r"(?:BEP-)?(\d+)", bep_identifier, re.IGNORECASE)
    if m:
        search_num = int(m.group(1))
    
    matches = []
    # Search in proposals dir
    candidates = list(PROPOSALS_DIR.glob("BEP-*"))
    # Legacy support for root level if any left
    candidates.extend(list(BEP_DIR.glob("BEP-*")))
    candidates.extend(list(DOCS_DIR.glob("BEP-*")))

    for d in candidates:
        if not d.is_dir(): continue
        
        # Check if directory name matches number
        m_dir = re.match(r"BEP-(\d+)", d.name)
        if m_dir and search_num is not None and int(m_dir.group(1)) == search_num:
            matches.append(d / "README.md")
            continue
            
        # Check if string contains identifier (case insensitive)
        if bep_identifier.lower() in d.name.lower():
                matches.append(d / "README.md")

    if len(matches) == 1:
        return matches[0]
    elif len(matches) > 1:
        print(f"Ambiguous identifier '{bep_identifier}'. Matches:")
        for m in matches:
            print(f"  - {m.parent.name}")
        sys.exit(1)
    
    return None

def update_frontmatter(path, updates):
    text = path.read_text(encoding="utf-8")
    
    if not text.startswith("---"):
        print(f"Error: No frontmatter found in {path}")
        sys.exit(1)
        
    frontmatter_end = text.find("---", 3)
    if frontmatter_end == -1:
        print(f"Error: Malformed frontmatter in {path}")
        sys.exit(1)
        
    frontmatter = text[3:frontmatter_end]
    body = text[frontmatter_end+3:] # keep the closing ---
    
    new_frontmatter_lines = []
    existing_keys = set()
    
    # Update existing keys
    for line in frontmatter.strip().splitlines():
        if ":" in line:
            key, val = line.split(":", 1)
            key = key.strip()
            existing_keys.add(key)
            
            if key in updates:
                new_frontmatter_lines.append(f"{key}: {updates[key]}")
            else:
                new_frontmatter_lines.append(line)
        else:
            new_frontmatter_lines.append(line)
            
    # Add new keys that weren't present
    for key, val in updates.items():
        if key not in existing_keys:
            new_frontmatter_lines.append(f"{key}: {val}")
            
    new_content = "---\n" + "\n".join(new_frontmatter_lines) + "\n---" + body
    path.write_text(new_content, encoding="utf-8")
    print(f"Updated {path.parent.name}")


def main():
    parser = argparse.ArgumentParser(description="Update a BEP proposal's status or touch it.")
    parser.add_argument("bep_id", help="The BEP ID or folder name (e.g. '001', 'BEP-001', 'BEP-001-exceptions')")
    parser.add_argument("--status", help=f"Set new status ({', '.join(VALID_STATUSES)})")
    parser.add_argument("--touch", action="store_true", help="Update the modification time (default if no other args)")
    
    args = parser.parse_args()
    
    target_path = find_bep_path(args.bep_id)
    if not target_path:
        print(f"Error: Could not find BEP matching '{args.bep_id}'")
        sys.exit(1)

    updates = {}
    
    if args.status:
        if args.status not in VALID_STATUSES:
            print(f"Error: Invalid status '{args.status}'. Must be one of: {', '.join(VALID_STATUSES)}")
            sys.exit(1)
        updates["status"] = args.status
        
    if updates:
        update_frontmatter(target_path, updates)
    
    # If touch is requested OR we made updates (which implicitly touches), we are good.
    # But if explicit --touch is requested, we ensure mtime is updated even if no content changed.
    # Python's write_text updates mtime, so updates handles it.
    # If only touch is requested (or nothing else), we touch.
    if args.touch or (not updates):
        target_path.touch()
        print(f"Touched {target_path.parent.name}")

if __name__ == "__main__":
    main()

