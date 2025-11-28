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


def _get_file_history(rel_path: str, base_branch: str = "canary") -> list:
    """
    Get list of commits between base_branch and HEAD that touched this file.
    Returns list of dicts: {'hash': str, 'short': str, 'subject': str}
    """
    file_path = f"beps/docs/{rel_path}"
    
    # format: hash|short_hash|subject
    log_output = _run_git([
        "log", 
        f"{base_branch}..HEAD", 
        "--format=%H|%h|%s", 
        "--", 
        file_path
    ])
    
    commits = []
    for line in log_output.splitlines():
        if not line.strip(): continue
        parts = line.split("|", 2)
        if len(parts) == 3:
            commits.append({
                "hash": parts[0],
                "short": parts[1],
                "subject": parts[2]
            })
    return commits


def _highlight_inline_changes_multi(markdown: str, diffs_by_ref: dict) -> str:
    """
    Add visual indicators for changed lines.
    Wraps lines in divs with data attributes for EACH ref that sees a change.
    diffs_by_ref: {'canary': {'added': [...]}, 'abc1234': {'added': [...]}, ...}
    """
    # If no diffs anywhere, return original
    if all(not d["added"] for d in diffs_by_ref.values()):
        return markdown
    
    lines = markdown.splitlines()
    result_lines = []
    i = 0
    
    while i < len(lines):
        line = lines[i]
        
        # Determine which refs consider this line "added"
        # Filter refs where this line is present in their 'added' list
        active_refs = []
        for ref, d in diffs_by_ref.items():
            # Fuzzy match (len > 3)
            if any(added in line and len(added) > 3 for added in d["added"]):
                active_refs.append(ref)
        
        if active_refs:
            # This line is changed in at least one view
            changed_block = [line]
            i += 1
            
            # Collect consecutive lines that share the EXACT SAME set of active refs
            while i < len(lines):
                next_line = lines[i]
                
                # Calculate active refs for next line
                next_active_refs = []
                for ref, d in diffs_by_ref.items():
                    if any(added in next_line and len(added) > 3 for added in d["added"]):
                        next_active_refs.append(ref)
                
                # Empty lines inherit the previous block's status for visual continuity
                is_empty = (next_line.strip() == "")
                
                if is_empty:
                    changed_block.append(next_line)
                    i += 1
                    continue

                # If the set of refs is exactly the same, group it
                if set(next_active_refs) == set(active_refs):
                    changed_block.append(next_line)
                    i += 1
                else:
                    break
            
            # Build data attributes
            # data-diff-canary="true" data-diff-abc123="true"
            attrs = ' '.join([f'data-diff-{ref}="true"' for ref in active_refs])
            
            # Wrap block
            result_lines.append(f'<div markdown="1" class="diff-wrapper" {attrs}>')
            result_lines.append("")
            result_lines.extend(changed_block)
            result_lines.append("")
            result_lines.append('</div>')
        else:
            result_lines.append(line)
            i += 1
    
    return "\n".join(result_lines)


def _get_diff_ui_assets() -> str:
    """
    Returns CSS and JS for the diff toggler.
    """
    return """
<style>
    /* Base wrapper style */
    .diff-wrapper {
        margin: 8px 0;
        border-left: 4px solid transparent;
        transition: border-color 0.2s, background-color 0.2s, padding-left 0.2s;
    }

    /* Active Diff State - Logic injected via dynamic styles per page */
    /* Pattern: body[data-diff-mode="REF"] .diff-wrapper[data-diff-REF="true"] { ... } */

    /* UI Controls */
    .diff-controls {
        position: sticky;
        top: 60px; /* Below header */
        z-index: 10;
        background: var(--md-default-bg-color);
        padding: 10px;
        border-bottom: 1px solid var(--md-default-fg-color--lightest);
        margin-bottom: 20px;
        display: flex;
        align-items: center;
        gap: 10px;
        font-size: 0.9rem;
    }
    
    .diff-select {
        padding: 4px 8px;
        border-radius: 4px;
        border: 1px solid var(--md-default-fg-color--light);
        background: var(--md-default-bg-color);
        color: var(--md-default-fg-color);
        max-width: 400px;
    }
    
    /* Navigation Indicators */
    .nav-diff-canary {
        display: none !important;
        margin-left: 6px;
        font-size: 0.8em;
    }
    
    body[data-diff-mode="canary"] .nav-diff-canary { 
        display: inline-block !important; 
    }
    
    /* Commit-specific blue dots (server-generated) */
    .nav-diff-commit {
        display: none !important;
        margin-left: 6px;
        font-size: 0.8em;
    }
    
    /* Dynamic CSS for each commit is injected per-page */
</style>

<script>
    (function() {
        function setDiffMode(mode) {
            document.body.setAttribute('data-diff-mode', mode);
            localStorage.setItem('bep-diff-mode', mode);
            
            // Update summaries
            document.querySelectorAll('.diff-summary').forEach(el => el.style.display = 'none');
            const activeSummary = document.getElementById('summary-' + mode);
            if (activeSummary) activeSummary.style.display = 'block';
            
            // Nav indicators are now handled purely by CSS!
            // Server-generated spans with data-commit attributes are shown/hidden via CSS rules
        }

        // Initialize dropdown state
        function initializeDropdown() {
            // Use requestAnimationFrame + setTimeout to ensure DOM is fully settled
            // This prevents "bouncing" when instant navigation replaces the sidebar
            requestAnimationFrame(() => {
                setTimeout(() => {
                    const storedMode = localStorage.getItem('bep-diff-mode') || 'canary';
                    const select = document.querySelector('.diff-select');
                    
                    if (select) {
                        // Check if stored mode is available in this page's dropdown
                        const hasOption = select.querySelector('option[value="'+storedMode+'"]');
                        
                        if (hasOption) {
                            // Mode is available, use it
                            select.value = storedMode;
                            setDiffMode(storedMode);
                        } else if (storedMode === 'canary' || storedMode === 'none') {
                            // Universal modes should always be available
                            select.value = 'canary';
                            setDiffMode('canary');
                        } else {
                            // Stored mode is a commit hash not on this page
                            // Keep canary as default
                            select.value = 'canary';
                            setDiffMode('canary');
                        }
                    } else {
                        // No dropdown on this page, just set mode for nav bar
                        const fallback = (storedMode === 'canary' || storedMode === 'none') ? storedMode : 'canary';
                        setDiffMode(fallback);
                    }
                }, 100); // 100ms delay to let MkDocs Material finish rendering
            });
        }

        // Run on initial load
        document.addEventListener('DOMContentLoaded', initializeDropdown);
        
        // Handle MkDocs Material instant navigation (AJAX page loads)
        // The theme emits this event when content changes
        document.addEventListener('DOMContentSwitch', initializeDropdown);
        
        // Also listen for Material's page navigation complete event
        // This fires AFTER the navigation has fully rendered
        document.addEventListener('location.changed', () => {
            setTimeout(initializeDropdown, 150);
        });
        
        // Fallback: Also run immediately in case script loads after DOM ready
        if (document.readyState === 'loading') {
            // Still loading, DOMContentLoaded will handle it
        } else {
            // DOM already loaded, run now
            initializeDropdown();
        }

        window.updateDiffMode = function(select) {
            setDiffMode(select.value);
        };
    })();
</script>
"""


def _run_diff(ref: str, file_path: str) -> str:
    # Helper to get diff vs a ref (hash or branch)
    # We compare Ref vs Current Working Tree
    # git diff REF -- file
    return _run_git(["diff", ref, "--", file_path])




def _add_diff_summary(markdown: str, rel_path: str) -> str:
    """
    Add diff summary and controls.
    """
    diff_canary = _diff_vs_branch(rel_path, base_branch="canary")
    diff_prev = _diff_vs_previous_commit(rel_path)
    
    # Count changes
    c_canary = len([l for l in diff_canary.splitlines() if l.startswith("+") or l.startswith("-")])
    c_prev = len([l for l in diff_prev.splitlines() if l.startswith("+") or l.startswith("-")])
    
    # Build Controls HTML
    controls = f"""
<div class="diff-controls">
    <label for="diff-mode"><strong>Compare against:</strong></label>
    <select id="diff-mode" class="diff-select" onchange="updateDiffMode(this)">
        <option value="canary">Canary Branch (Main)</option>
        <option value="prev">Previous Commit</option>
        <option value="none">None (Clean View)</option>
    </select>
</div>
"""
    
    # Summaries
    summary_html = ""
    if c_canary > 0:
        summary_html += f'<div id="summary-canary" style="display:none" markdown="1">\n\n!!! info "Diff vs Canary"\n    {c_canary} lines changed\n</div>\n'
    else:
         summary_html += f'<div id="summary-canary" style="display:none" markdown="1">\n\n!!! success "Diff vs Canary"\n    No changes vs canary\n</div>\n'

    if c_prev > 0:
        summary_html += f'<div id="summary-prev" style="display:none" markdown="1">\n\n!!! info "Diff vs Previous"\n    {c_prev} lines changed\n</div>\n'
    else:
         summary_html += f'<div id="summary-prev" style="display:none" markdown="1">\n\n!!! success "Diff vs Previous"\n    No changes vs previous commit\n</div>\n'

    return _get_diff_ui_assets() + controls + "\n" + summary_html + "\n" + markdown



def on_nav(nav, config, files, **kwargs):
    """
    MkDocs hook: runs after navigation is created.
    - Scans all proposals for history.
    - Injects server-generated indicators (green & blue dots) into nav titles.
    - Blue dots are toggled via CSS based on selected commit.
    """
    def walk_nav(items):
        for item in items:
            # Check if it's a Page object (has 'file' attribute)
            if getattr(item, "file", None):
                rel_path = item.file.src_path
                
                # Only check proposals
                if rel_path.startswith("proposals/"):
                    # 1. Canary Check (Static)
                    diff_canary = _diff_vs_branch(rel_path, base_branch="canary")
                    has_canary = bool(diff_canary.strip())
                    
                    # 2. History Check
                    # Get last 20 commits for this file
                    commits = _get_file_history(rel_path, base_branch="canary")
                    # Limit to 20 to match page logic
                    commits = commits[:20]

                    # 3. Update Title
                    # If title is not set yet, infer it from filename
                    if not item.title:
                        stem = Path(rel_path).stem
                        if stem == "README":
                            stem = Path(rel_path).parent.name
                        item.title = stem.replace("-", " ").title()
                    
                    # Sanitize path for ID (replace / and . with -)
                    safe_path = rel_path.replace("/", "-").replace(".", "-")
                    
                    # Build indicators (all server-generated!)
                    indicators = ""
                    
                    # Green dot for Canary
                    if has_canary:
                        indicators += '<span class="nav-diff-canary">🟢</span>'
                    
                    # Blue dots for each commit where there are ACTUAL changes
                    for c in commits:
                        h = c["hash"]
                        # Check if there are actual differences vs this commit
                        diff_vs_commit = _run_git(["diff", h, "--", f"beps/docs/{rel_path}"])
                        if diff_vs_commit.strip():
                            # Only add blue dot if there are changes
                            indicators += f'<span class="nav-diff-commit" data-commit="{h}">🔵</span>'
                    
                    item.title = f'<span id="nav-item-{safe_path}" class="bep-nav-item" data-path="{rel_path}">{item.title}</span>{indicators}'
            
            # Check if it's a Section (has 'children' attribute)
            if getattr(item, "children", None):
                walk_nav(item.children)
    
    walk_nav(nav.items)
    return nav


def on_page_markdown(markdown: str, page, **kwargs) -> str:
    """
    MkDocs hook: runs for every page render.
    - Fetches history
    - Generates diffs vs Canary and vs Commits
    - Injects generic highlighting
    - Injects Dropdown + CSS + Summaries
    """
    if not page or not getattr(page, "file", None) or not page.file.src_path:
        return markdown

    rel_path = page.file.src_path  # relative to docs/, e.g. 'proposals/.../go.md'

    # Optional: only show diffs for proposals
    if not rel_path.startswith("proposals/"):
        return markdown

    # 1. Get History
    commits = _get_file_history(rel_path, base_branch="canary")
    # Limit history to prevent performance explosion? user said "every commit"
    # but 50 diffs is a lot. Let's try 20.
    commits = commits[:20]

    # 2. Collect Diffs
    # We always include Canary
    diffs = {}
    
    # Canary
    d_canary = _run_git(["diff", "canary", "--", f"beps/docs/{rel_path}"])
    parsed_canary = _parse_unified_diff(d_canary)
    diffs["canary"] = parsed_canary
    
    # Commits
    for c in commits:
        # diff vs that commit (commit..HEAD logic roughly, but here we diff commit vs working tree)
        d_text = _run_git(["diff", c["hash"], "--", f"beps/docs/{rel_path}"])
        diffs[c["hash"]] = _parse_unified_diff(d_text)
        
    # 3. Highlight Inline
    highlighted_markdown = _highlight_inline_changes_multi(markdown, diffs)
    
    # If no diffs anywhere, just return (but we might want to show the dropdown saying "No changes"?)
    # Actually if there are changes vs SOMETHING we should show UI.
    has_any_change = any(bool(d["added"] or d["removed"]) for d in diffs.values())

    if not has_any_change:
        return markdown

    # 4. Generate Dynamic CSS & UI
    
    # CSS for Canary
    css_rules = []
    css_rules.append("""
    body[data-diff-mode="canary"] .diff-wrapper[data-diff-canary="true"] {
        padding-left: 16px;
        border-left-color: #acf2bd; /* Green */
        background-color: rgba(172, 242, 189, 0.1);
    }
    """)
    
    # CSS for Commits (Blue)
    for c in commits:
        h = c["hash"]
        css_rules.append(f"""
    body[data-diff-mode="{h}"] .diff-wrapper[data-diff-{h}="true"] {{
        padding-left: 16px;
        border-left-color: #a5d6ff; /* Blue */
        background-color: rgba(165, 214, 255, 0.1);
    }}
    body[data-diff-mode="{h}"] .nav-diff-commit[data-commit="{h}"] {{
        display: inline-block !important;
    }}
        """)
        
    dynamic_style = "<style>" + "\n".join(css_rules) + "</style>"
    
    # Dropdown Options
    options = []
    options.append('<option value="canary">Canary Branch (Main)</option>')
    
    for c in commits:
        label = f"Commit {c['short']}: {c['subject']}"
        # Truncate if too long
        if len(label) > 80: label = label[:77] + "..."
        options.append(f'<option value="{c["hash"]}">{label}</option>')
        
    options.append('<option value="none">None (Clean View)</option>')
    
    controls = f"""
<div class="diff-controls">
    <label for="diff-mode"><strong>Compare against:</strong></label>
    <select id="diff-mode" class="diff-select" onchange="updateDiffMode(this)">
        {"".join(options)}
    </select>
</div>
"""

    # Summaries
    summaries = []
    # Canary Summary
    c_canary = len(parsed_canary["added"]) + len(parsed_canary["removed"])
    if c_canary > 0:
        summaries.append(f'<div id="summary-canary" class="diff-summary" style="display:none" markdown="1">\n\n!!! info "Diff vs Canary"\n    {c_canary} lines changed\n</div>')
    else:
        summaries.append(f'<div id="summary-canary" class="diff-summary" style="display:none" markdown="1">\n\n!!! success "Diff vs Canary"\n    No changes vs canary\n</div>')

    # Commit Summaries
    for c in commits:
        h = c["hash"]
        p = diffs[h]
        count = len(p["added"]) + len(p["removed"])
        if count > 0:
             summaries.append(f'<div id="summary-{h}" class="diff-summary" style="display:none" markdown="1">\n\n!!! info "Diff vs {c["short"]}"\n    {count} lines changed\n</div>')
        else:
             summaries.append(f'<div id="summary-{h}" class="diff-summary" style="display:none" markdown="1">\n\n!!! success "Diff vs {c["short"]}"\n    No changes vs {c["short"]}\n</div>')

    summary_block = "\n".join(summaries)

    # Assemble final page
    return _get_diff_ui_assets() + dynamic_style + controls + "\n" + summary_block + "\n" + highlighted_markdown
