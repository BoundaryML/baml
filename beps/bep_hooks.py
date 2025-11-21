import subprocess
from pathlib import Path

# This file lives in .../baml-3/beps/bep_hooks.py
# Repo root is one level up from beps/
REPO_ROOT = Path(__file__).resolve().parents[1]
DOCS_DIR = Path(__file__).resolve().parent / "docs"


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
    file_path = f"beps/docs/{rel_path}"
    # Includes uncommitted changes vs the given branch
    return _run_git(["diff", base_branch, "--", file_path])


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


def _indent_for_admonition(text: str) -> str:
    """Indent every line by 4 spaces so it sits inside an admonition."""
    lines = text.rstrip("\n").splitlines()
    return "\n".join("    " + line for line in lines)


def _build_diff_block(title: str, diff_text: str) -> str:
    if not diff_text.strip():
        return ""
    indented = _indent_for_admonition(diff_text)
    return (
        f'???+ info "{title}"\n\n'
        f"    ```diff\n"
        f"{indented}\n"
        f"    ```\n"
    )


def on_page_markdown(markdown: str, page, **kwargs) -> str:
    """
    MkDocs hook: runs for every page render (including mkdocs serve rebuilds).
    """
    if not page or not getattr(page, "file", None) or not page.file.src_path:
        return markdown

    rel_path = page.file.src_path  # relative to docs/, e.g. 'proposals/.../go.md'

    # Optional: only show diffs for proposals
    if not rel_path.startswith("proposals/"):
        return markdown

    diff_main = _diff_vs_branch(rel_path, base_branch="canary")
    diff_prev = _diff_vs_previous_commit(rel_path)

    extra = ""
    extra += _build_diff_block("Changes vs canary", diff_main)
    extra += _build_diff_block("Changes vs previous commit", diff_prev)

    if not extra:
        return markdown

    # Append to the end of the page
    return markdown.rstrip() + "\n\n" + extra + "\n"
