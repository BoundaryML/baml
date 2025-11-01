# Git Hooks Setup

This repository includes Git hooks to ensure code quality before commits.

## Setup

Run the setup script to install the hooks:

```bash
./scripts/setup-hooks.sh
```

## What the pre-commit hook does

The pre-commit hook automatically runs before each commit and:

1. **Runs `cargo fmt --all`** in the `baml_language` directory to automatically format code
2. **Runs `cargo clippy --fix`** to automatically fix clippy warnings where possible
3. **Stages any auto-fixed changes** to include them in your commit
4. **Verifies all checks pass** after auto-fixes are applied

If there are clippy warnings that can't be auto-fixed, the commit will be aborted with instructions.

## Bypassing the hooks (escape hatches)

Sometimes you need to commit work-in-progress or have a valid reason to skip the checks. You have three options:

### Option 1: Use --no-verify flag
```bash
git commit --no-verify -m "WIP: experimental changes"
```

### Option 2: Add [skip-checks] to commit message
```bash
git commit -m "WIP: testing something [skip-checks]"
```

### Option 3: Set environment variable
```bash
SKIP_CHECKS=1 git commit -m "WIP: quick save"
```

## Manual fixes

The hook automatically fixes most issues, but if it fails on clippy warnings that can't be auto-fixed:

### For remaining clippy warnings:
Review the clippy output and fix the warnings manually. Common issues that need manual fixes:
- Logic errors or potential bugs
- Performance issues that require refactoring
- Missing documentation on public items
- Complex type inference issues
- Unsafe code that needs review

## Troubleshooting

If the hook isn't running:
1. Make sure you've run `./scripts/setup-hooks.sh`
2. Check that `.git/hooks/pre-commit` exists and is executable
3. Ensure you're committing from the repository root

## Disabling hooks permanently (not recommended)

If you need to disable hooks for your local development:
```bash
git config core.hooksPath /dev/null
```

To re-enable:
```bash
git config --unset core.hooksPath
./scripts/setup-hooks.sh
```