#!/usr/bin/env -S uv run --quiet --script
# /// script
# dependencies = []
# ///
"""
Validates that only README.md files or whitelisted .md files exist in baml_language/
"""

import subprocess
import sys
from pathlib import Path


def load_whitelist(whitelist_file: Path) -> list[str]:
    """Load whitelist patterns from file, ignoring comments and empty lines."""
    patterns = []
    with open(whitelist_file) as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith("#"):
                patterns.append(line)
    return patterns


def matches_pattern(path: str, pattern: str) -> bool:
    """Check if path matches a glob pattern."""
    from fnmatch import fnmatch
    return fnmatch(path, pattern)


def main() -> int:
    import subprocess

    baml_language_dir = Path(__file__).parent.parent
    whitelist_file = baml_language_dir / ".markdown-whitelist"

    # Find all .md files tracked by git (respects .gitignore, excludes deleted files)
    try:
        result = subprocess.run(
            ["git", "ls-files", "--exclude-standard", "--cached", "*.md", "**/*.md"],
            cwd=baml_language_dir,
            capture_output=True,
            text=True,
            check=True,
        )
        all_files = [line for line in result.stdout.strip().split("\n") if line]

        # Exclude files that are staged for deletion
        deleted_result = subprocess.run(
            ["git", "diff", "--name-only", "--diff-filter=D", "--cached"],
            cwd=baml_language_dir,
            capture_output=True,
            text=True,
            check=True,
        )
        deleted_files = set(line for line in deleted_result.stdout.strip().split("\n") if line)

        md_files = sorted(f for f in all_files if f not in deleted_files)
    except subprocess.CalledProcessError:
        print("ERROR: Failed to list git tracked files")
        return 1

    # Load whitelist patterns
    whitelist_patterns = load_whitelist(whitelist_file)

    # Check each .md file
    violations = []
    for md_file in md_files:
        # Allow all README.md files
        if Path(md_file).name == "README.md":
            continue

        # Check if matches any whitelist pattern
        if any(matches_pattern(md_file, pattern) for pattern in whitelist_patterns):
            continue

        violations.append(md_file)

    # Report violations
    if violations:
        print("ERROR: Found markdown files that are not README.md and not in the whitelist:")
        print()
        for violation in violations:
            print(f"  - {violation}")
        print()
        print("Only long-term informational .md files should be checked in, otherwise please ensure you meant to check them in by updating the whitelist file")
        print()
        print("To explicitly allow these files, add them to: baml_language/.markdown-whitelist")
        return 1

    print("✓ All markdown files are valid (README.md or whitelisted)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
