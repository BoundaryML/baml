import subprocess
import re
from pathlib import Path

# This file lives in .../baml-3/beps/bep_hooks.py
# Repo root is one level up from beps/
REPO_ROOT = Path(__file__).resolve().parents[1]
DOCS_DIR = Path(__file__).resolve().parent / "docs"

# Cache for git diff results to avoid re-running for nav and page content
_DIFF_CACHE = {}


def _run_git(args: list[str]) -> str:
    """Run git in the repo root and return stdout (or empty on error)."""
    try:
        result = subprocess.run(
            ["git"] + args,
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
    except Exception:
        return ""
    if result.returncode != 0:
        return ""
    return result.stdout


def _diff_vs_branch(rel_path: str, base_branch: str = "main") -> str:
    """
    Diff current working tree vs base_branch for this file.

    rel_path is like: 'proposals/BEP-001-exceptions/context/go.md'
    """
    # Check cache first
    cache_key = f"{rel_path}:{base_branch}"
    if cache_key in _DIFF_CACHE:
        return _DIFF_CACHE[cache_key]

    file_path = f"beps/docs/{rel_path}"
    # Includes uncommitted changes vs the given branch
    result = _run_git(["diff", base_branch, "--", file_path])
    
    # Cache result
    _DIFF_CACHE[cache_key] = result
    return result


def _diff_vs_previous_commit(rel_path: str) -> str:
    """
    Diff last commit vs previous commit for this file.
    """
    file_path = f"beps/docs/{rel_path}"
    log_output = _run_git(
        ["log", "-n", "2", "--pretty=format:%H", "--", file_path]
    )
    commits = [line.strip() for line in log_output.splitlines() if line.strip()]
    if len(commits) < 2:
        return ""
    head, prev = commits[0], commits[1]
    return _run_git(["diff", prev, head, "--", file_path])


def _parse_unified_diff(diff_text: str) -> dict:
    """
    Parse unified diff and extract line changes.
    Returns dict with 'added' and 'removed' line contents.
    """
    if not diff_text.strip():
        return {"added": [], "removed": [], "modified": []}
    
    added = []
    removed = []
    
    for line in diff_text.splitlines():
        # Skip diff metadata lines
        if line.startswith("+++") or line.startswith("---") or \
           line.startswith("@@") or line.startswith("diff ") or \
           line.startswith("index ") or line.startswith("new file") or \
           line.startswith("old file"):
            continue
        
        if line.startswith("+"):
            # Added line (strip the + prefix)
            content = line[1:].strip()
            if content:  # Skip empty additions
                added.append(content)
        elif line.startswith("-"):
            # Removed line (strip the - prefix)
            content = line[1:].strip()
            if content:  # Skip empty removals
                removed.append(content)
    
    return {"added": added, "removed": removed}


def _highlight_inline_changes(markdown: str, diff_changes: dict) -> str:
    """
    Add visual indicators for changed lines using vertical bars.
    Uses markdown="1" attribute to preserve markdown rendering inside divs.
    """
    if not diff_changes["added"] and not diff_changes["removed"]:
        return markdown
    
    lines = markdown.splitlines()
    result_lines = []
    i = 0
    
    while i < len(lines):
        line = lines[i]
        
        # Check if this line contains added content
        has_addition = any(
            added_content in line and len(added_content) > 3
            for added_content in diff_changes["added"]
        )
        
        if has_addition:
            # Start a block of changed content
            changed_block = []
            changed_block.append(line)
            i += 1
            
            # Continue collecting consecutive changed lines or empty lines
            while i < len(lines):
                next_line = lines[i]
                has_next_addition = any(
                    added_content in next_line and len(added_content) > 3
                    for added_content in diff_changes["added"]
                )
                
                if has_next_addition or (next_line.strip() == "" and i + 1 < len(lines)):
                    changed_block.append(next_line)
                    i += 1
                else:
                    break
            
            # Wrap the changed block with a div that allows markdown processing
            result_lines.append('<div markdown="1" style="border-left: 4px solid #acf2bd; padding-left: 16px; margin: 8px 0;">')
            result_lines.append("")
            result_lines.extend(changed_block)
            result_lines.append("")
            result_lines.append('</div>')
        else:
            result_lines.append(line)
            i += 1
    
    return "\n".join(result_lines)


def _add_diff_summary(markdown: str, rel_path: str, base_branch: str = "canary") -> str:
    """
    Add a subtle diff summary at the top of the page.
    """
    diff_main = _diff_vs_branch(rel_path, base_branch=base_branch)
    diff_prev = _diff_vs_previous_commit(rel_path)
    
    if not diff_main.strip() and not diff_prev.strip():
        return markdown
    
    # Count changes
    changes_main = len([l for l in diff_main.splitlines() if l.startswith("+") or l.startswith("-")])
    changes_prev = len([l for l in diff_prev.splitlines() if l.startswith("+") or l.startswith("-")])
    
    summary_parts = []
    if changes_main > 0:
        summary_parts.append(f"{changes_main} lines changed vs {base_branch}")
    if changes_prev > 0:
        summary_parts.append(f"{changes_prev} lines changed in last commit")
    
    if summary_parts:
        summary = " | ".join(summary_parts)
        notice = f'\n!!! info "Diff Summary"\n    {summary}\n\n'
        return notice + markdown
    
    return markdown


def on_nav(nav, config, files, **kwargs):
    """
    MkDocs hook: runs after navigation is created.
    Adds a green dot to pages that have changes vs canary.
    """
    def walk_nav(items):
        for item in items:
            # Check if it's a Page object (has 'file' attribute)
            if getattr(item, "file", None):
                rel_path = item.file.src_path
                
                # Only check proposals
                if rel_path.startswith("proposals/"):
                    diff = _diff_vs_branch(rel_path, base_branch="canary")
                    if diff.strip():
                        # If title is not set yet, infer it from filename
                        if not item.title:
                            # Basic inference: use filename stem, replace - with spaces, title case
                            # This mimics standard MkDocs behavior
                            stem = Path(rel_path).stem
                            if stem == "README":
                                # Use parent directory name
                                stem = Path(rel_path).parent.name
                            
                            # Remove BEP-XXX prefix if present for cleaner title
                            # Or keep it if preferred. MkDocs usually keeps it.
                            item.title = stem.replace("-", " ").title()

                        # Add green dot to title
                        item.title = f"{item.title} 🟢"
            
            # Check if it's a Section (has 'children' attribute)
            if getattr(item, "children", None):
                walk_nav(item.children)
    
    walk_nav(nav.items)
    return nav


def on_page_markdown(markdown: str, page, **kwargs) -> str:
    """
    MkDocs hook: runs for every page render (including mkdocs serve rebuilds).
    Highlights changed content inline with green background.
    """
    if not page or not getattr(page, "file", None) or not page.file.src_path:
        return markdown

    rel_path = page.file.src_path  # relative to docs/, e.g. 'proposals/.../go.md'

    # Optional: only show diffs for proposals
    if not rel_path.startswith("proposals/"):
        return markdown

    # Get diffs
    diff_main = _diff_vs_branch(rel_path, base_branch="canary")
    
    if not diff_main.strip():
        return markdown
    
    # Parse the diff to find what changed
    changes = _parse_unified_diff(diff_main)
    
    # Apply inline highlighting
    highlighted_markdown = _highlight_inline_changes(markdown, changes)
    
    # Add subtle summary at top
    final_markdown = _add_diff_summary(highlighted_markdown, rel_path, base_branch="canary")
    
    return final_markdown
