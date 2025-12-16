import subprocess
import re
from pathlib import Path

# This file lives in .../baml-3/beps/bep_hooks.py
# Repo root is one level up from beps/
REPO_ROOT = Path(__file__).resolve().parents[1]
DOCS_DIR = Path(__file__).resolve().parent / "docs"

# Cache for git diff results to avoid re-running for nav and page content
_DIFF_CACHE = {}


def _generate_toc(markdown: str) -> str:
    """
    Replace <!-- TOC_PLACEHOLDER --> placeholder with auto-generated table of contents.
    Extracts ## sections and ### questions to build a linked TOC.
    """
    if "<!-- TOC_PLACEHOLDER -->" not in markdown:
        return markdown
    
    toc_lines = []
    current_section = None
    
    for line in markdown.splitlines():
        # Match ## Section headers (but skip ## Contents if present)
        if line.startswith("## ") and "Contents" not in line:
            current_section = line[3:].strip()
            toc_lines.append(f"\n**{current_section}**\n")
        # Match ### Question headers
        elif line.startswith("### "):
            title = line[4:].strip()
            # Generate anchor: lowercase, spaces to dashes, remove backticks and special chars
            anchor = title.lower()
            anchor = anchor.replace(" ", "-")
            anchor = anchor.replace("`", "")
            anchor = anchor.replace("?", "")
            anchor = anchor.replace("(", "")
            anchor = anchor.replace(")", "")
            anchor = anchor.replace("/", "")
            anchor = anchor.replace("'", "")
            anchor = re.sub(r"-+", "-", anchor)  # Collapse multiple dashes
            anchor = anchor.strip("-")
            toc_lines.append(f"- [{title}](#{anchor})")
    
    toc = "\n".join(toc_lines)
    return markdown.replace("<!-- TOC_PLACEHOLDER -->", toc)


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


def _parse_unified_diff(diff_text: str) -> set[int]:
    """
    Parse unified diff and return a set of 1-based line numbers 
    in the NEW file that are added or modified.
    """
    changed_lines = set()
    if not diff_text.strip():
        return changed_lines
    
    # Initialize for tracking line numbers in the new file
    current_new_line = 0

    for line in diff_text.splitlines():
        # Skip diff metadata lines
        if line.startswith("@@"):
            # Parse header: @@ -1,5 +1,5 @@
            # We care about the +part
            # +1,5 means starts at line 1, 5 lines
            try:
                # Extract the + part
                parts = line.split(" ")
                new_hunk = parts[2] # +1,5
                if "," in new_hunk:
                    start = int(new_hunk[1:].split(",")[0])
                else:
                    start = int(new_hunk[1:])
                current_new_line = start
                # print(f"DEBUG: Hunk start {start}")
            except Exception:
                # print(f"DEBUG: Hunk parse error {e}")
                pass
            continue
        
        if line.startswith("+++") or line.startswith("---") or \
           line.startswith("diff ") or line.startswith("index ") or \
           line.startswith("new file") or line.startswith("old file"):
            continue
        
        if line.startswith("+"):
            # Added line
            changed_lines.add(current_new_line)
            current_new_line += 1
        elif line.startswith("-"):
            # Removed line - doesn't exist in new file, so doesn't advance current_new_line
            pass
        else:
            # Context line (starts with space)
            current_new_line += 1
    
    # print(f"DEBUG: Total changed lines: {len(changed_lines)}")
    return changed_lines


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
    Wraps lines in Admonitions for EACH ref that sees a change.
    diffs_by_ref: {'canary': set([1, 2, 5]), 'abc1234': set([1]), ...}
    """
    # If no diffs anywhere, return original
    if all(not d for d in diffs_by_ref.values()):
        return markdown
    
    lines = markdown.splitlines()
    result_lines = []
    i = 0
    
    # 1-based line index
    current_line_num = 1
    
    # Track if we're inside a code fence
    in_code_fence = False
    code_fence_marker = None
    
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        
        # 1. Track Code Fences
        if stripped.startswith('```') or stripped.startswith('~~~'):
            if not in_code_fence:
                in_code_fence = True
                code_fence_marker = '```' if stripped.startswith('```') else '~~~'
            elif (code_fence_marker == '```' and stripped.startswith('```')) or \
                 (code_fence_marker == '~~~' and stripped.startswith('~~~')):
                in_code_fence = False
                code_fence_marker = None
        
        # 2. Determine active refs for this line
        active_refs = []
        for ref, changed_lines_set in diffs_by_ref.items():
            if current_line_num in changed_lines_set:
                active_refs.append(ref)
        
        # 3. Check if line is safe to highlight (Root-level content only)
        # Skip:
        # - Inside code fences
        # - Indented lines (start with space/tab) -> likely blockquotes, lists, or code
        # - List items (start with -, *, +, 1.)
        # - Tables (start with |)
        # - Empty lines (don't wrap empty lines alone)
        
        is_safe = True
        if in_code_fence:
            is_safe = False
        elif stripped.startswith('```') or stripped.startswith('~~~'): # Fence markers themselves (opening or closing)
            is_safe = False
        elif not line: # Empty line
            is_safe = False
        elif line.startswith(' ') or line.startswith('\t'): # Indented
            is_safe = False
        elif line.startswith('|'): # Table
            is_safe = False
        elif stripped.startswith('- ') or stripped.startswith('* ') or stripped.startswith('+ '): # Unordered list
            is_safe = False
        elif stripped[0].isdigit() and '. ' in stripped[:5]: # Ordered list (heuristic)
            is_safe = False
            
        # 4. Wrap if active and safe
        if active_refs and is_safe:
            # This line is changed in at least one view
            changed_block = [line]
            i += 1
            current_line_num += 1
            
            # Collect consecutive lines that share the EXACT SAME set of active refs
            # AND are also safe to highlight
            while i < len(lines):
                next_line = lines[i]
                next_stripped = next_line.strip()
                
                # Check fence state for next line
                next_in_fence = in_code_fence
                if next_stripped.startswith('```') or next_stripped.startswith('~~~'):
                    if not in_code_fence:
                        next_in_fence = True
                    elif (code_fence_marker == '```' and next_stripped.startswith('```')) or \
                         (code_fence_marker == '~~~' and next_stripped.startswith('~~~')):
                        next_in_fence = False
                
                # Check safety for next line
                next_is_safe = True
                if next_in_fence:
                    next_is_safe = False
                elif not next_line:
                    next_is_safe = False
                elif next_line.startswith(' ') or next_line.startswith('\t'):
                    next_is_safe = False
                elif next_line.startswith('|'):
                    next_is_safe = False
                elif next_stripped.startswith('- ') or next_stripped.startswith('* ') or next_stripped.startswith('+ '):
                    next_is_safe = False
                elif next_stripped and next_stripped[0].isdigit() and '. ' in next_stripped[:5]:
                    next_is_safe = False

                # Calculate active refs for next line
                next_active_refs = []
                for ref, changed_lines_set in diffs_by_ref.items():
                    if current_line_num in changed_lines_set:
                        next_active_refs.append(ref)
                
                # Group if refs match AND next line is safe
                if set(next_active_refs) == set(active_refs) and next_is_safe:
                    changed_block.append(next_line)
                    i += 1
                    current_line_num += 1
                    
                    # Update fence state
                    if next_stripped.startswith('```') or next_stripped.startswith('~~~'):
                        if not in_code_fence:
                            in_code_fence = True
                            code_fence_marker = '```' if next_stripped.startswith('```') else '~~~'
                        elif (code_fence_marker == '```' and next_stripped.startswith('```')) or \
                             (code_fence_marker == '~~~' and next_stripped.startswith('~~~')):
                            in_code_fence = False
                            code_fence_marker = None
                else:
                    break
            
            # Use Admonition syntax!
            # We use distinct classes for each ref to avoid attribute issues
            # !!! diff-canary ""
            # !!! diff-abc1234 ""
            
            # If multiple refs active, we just pick the first one for the class name
            # The CSS handles showing/hiding based on body class
            # But wait, if we have multiple refs, we need a class that matches the CURRENTLY VIEWED ref.
            # Since we can't easily put multiple classes on an admonition without attributes (which failed),
            # we will generate a GENERIC class `diff-block` and add specific classes if possible,
            # OR we generate `!!! diff-canary` if canary is active, etc.
            # But what if both canary and commit X are active?
            # We need the block to be visible if EITHER is selected.
            
            # Solution: Generate a generic `!!! diff-block ""`
            # And inject a `<div style="display:none" data-diff-refs="canary,abc1234"></div>` inside? No.
            
            # Let's go back to the attribute attempt but fix the syntax.
            # `!!! note "" {: .diff-block .diff-canary .diff-abc1234 }`
            # This requires `attr_list` extension.
            # If that failed, we can try the standard class syntax:
            # `!!! note "Title" class="diff-block"` -> No that's not standard.
            
            # Alternative: Just wrap in a raw div again but use `markdown="1"` properly?
            # We tried that and it failed (horizontal layout).
            
            # Let's try the distinct admonition type again.
            # If we have multiple refs, we can nest them? No.
            # We can output multiple admonitions? No, duplicates content.
            
            # Best bet: Use `!!! diff-block` and rely on the fact that we only care about
            # the currently selected mode.
            # But we need to know WHICH refs are active for this block to show/hide it.
            
            # Actually, the previous CSS logic was:
            # body[data-diff-mode="canary"] .diff-wrapper[data-diff-canary="true"]
            
            # If we can't use attributes, we can't selectively show/hide blocks that are ONLY changed in canary vs ONLY changed in commit X.
            # Wait, if a block is changed in BOTH, it should show in BOTH.
            
            # Let's try the attribute syntax one more time but simpler.
            # `!!! diff-block ""` 
            # `    {: .diff-canary .diff-abc1234 }`
            # putting the attribute list on the content? No.
            
            # Let's try to fix the attribute list syntax.
            # The screenshot showed `!!! note "" {: data-diff-canary="true" }` literally.
            # This means `attr_list` is NOT processing it on the admonition line.
            # `attr_list` usually works on headers, images, lists.
            # For admonitions, it might not be supported directly on the opening line in all versions.
            
            # WORKAROUND:
            # Use a raw `<div>` wrapper AROUND the admonition?
            # <div class="diff-container" data-diff-canary="true">
            # !!! diff-block ""
            #     Content
            # </div>
            # This requires `md_in_html` which we have.
            
            attrs = ' '.join([f'data-diff-{ref}="true"' for ref in active_refs])
            
            result_lines.append("")
            result_lines.append(f'<div class="diff-container" {attrs} markdown="1">')
            result_lines.append(f'!!! diff-block ""')
            for block_line in changed_block:
                result_lines.append(f'    {block_line}')
            result_lines.append(f'</div>')
            result_lines.append("")
        else:
            result_lines.append(line)
            i += 1
            current_line_num += 1
    
    return "\n".join(result_lines)


def _get_diff_ui_assets() -> str:
    """
    Returns CSS and JS for the diff toggler.
    """
    return """
<style>
    /* Container style - handles visibility */
    .diff-container {
        /* Default: transparent/visible? No, we need to hide if not active in current mode */
        /* Actually, we can't easily hide the container based on body class if we don't have the attribute.
           But we DO have the attribute on the container now! */
    }

    /* Admonition style */
    /* Admonition style */
    .md-typeset .admonition.diff-block {
        margin: 0; /* Let container handle margin */
        border: none; /* Remove all borders */
        border-left: 4px solid transparent; /* We'll add back left border for highlighting */
        border-radius: 0;
        box-shadow: none;
        padding: 0; /* No padding by default */
        background: transparent;
        font-size: inherit;
        overflow: visible;
        display: block; /* Override flow-root to allow margin collapsing */
    }
    
    .md-typeset .admonition.diff-block > .admonition-title {
        display: none;
    }
    
    .md-typeset .admonition.diff-block > .md-typeset {
        padding: 0;
    }

    /* Active Diff State */
    
    /* None mode - no highlighting */
    body[data-diff-mode="none"] .diff-container .admonition.diff-block {
        border-left-color: transparent;
        background-color: transparent;
        padding-left: 0; /* Explicitly no padding */
        border: none !important; /* Remove border to allow margin collapsing */
    }
    
    /* Canary */
    body[data-diff-mode="canary"] .diff-container[data-diff-canary="true"] .admonition.diff-block {
        border-left-color: #acf2bd;
        background-color: rgba(172, 242, 189, 0.1);
        padding-left: 12px; /* Add padding when highlighting */
    }
    
    /* Commits - Dynamic rules injected below */

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

    # EMERGENCY FIX: Disable hook entirely to fix empty pages
    # return markdown

    # Optional: only show diffs for proposals
    if not rel_path.startswith("proposals/"):
        return markdown
    
    # Generate TOC from <!-- TOC_PLACEHOLDER --> placeholder
    markdown = _generate_toc(markdown)
    
    # DEBUG: Skip go.md to see if it renders
    # if "go.md" in rel_path:
    #    return markdown

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
    has_any_change = any(bool(d) for d in diffs.values())

    if not has_any_change:
        return markdown

    # 4. Generate Dynamic CSS & UI
    
    # CSS for Canary
    css_rules = []
    css_rules.append("""
    body[data-diff-mode="canary"] .diff-container[data-diff-canary="true"] .admonition.diff-block {
        border-left-color: #acf2bd; /* Green */
        background-color: rgba(172, 242, 189, 0.1);
        padding-left: 12px; /* Add padding when highlighting */
    }
    """)
    
    # CSS for Commits (Blue)
    for c in commits:
        h = c["hash"]
        css_rules.append(f"""
    body[data-diff-mode="{h}"] .diff-container[data-diff-{h}="true"] .admonition.diff-block {{
        border-left-color: #a5d6ff; /* Blue */
        background-color: rgba(165, 214, 255, 0.1);
        padding-left: 12px; /* Add padding when highlighting */
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
    c_canary = len(parsed_canary)
    if c_canary > 0:
        summaries.append(f'<div id="summary-canary" class="diff-summary" style="display:none" markdown="1">\n\n!!! info "Diff vs Canary"\n    {c_canary} lines changed\n</div>')
    else:
        summaries.append(f'<div id="summary-canary" class="diff-summary" style="display:none" markdown="1">\n\n!!! success "Diff vs Canary"\n    No changes vs canary\n</div>')

    # Commit Summaries
    for c in commits:
        h = c["hash"]
        p = diffs[h]
        count = len(p)
        if count > 0:
             summaries.append(f'<div id="summary-{h}" class="diff-summary" style="display:none" markdown="1">\n\n!!! info "Diff vs {c["short"]}"\n    {count} lines changed\n</div>')
        else:
             summaries.append(f'<div id="summary-{h}" class="diff-summary" style="display:none" markdown="1">\n\n!!! success "Diff vs {c["short"]}"\n    No changes vs {c["short"]}\n</div>')

    summary_block = "\n".join(summaries)

    # Assemble final page
    return _get_diff_ui_assets() + dynamic_style + controls + "\n" + summary_block + "\n" + highlighted_markdown
