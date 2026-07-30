# Git Hooks Setup

This repository uses [prek](https://prek.j178.dev/) for pre-commit hooks.

## Setup

Install hooks with prek:

```bash
prek install
```

## What the pre-commit hook does

The pre-commit hook automatically runs before each commit and:

1. **Runs `cargo fmt --all`** in the `baml_language` directory to automatically format code
2. **Runs `cargo clippy --fix`** to automatically fix clippy warnings where possible
3. **Runs `cargo stow --check`** to validate workspace crate organization
4. **Updates BEP README** if any BEP files changed

If there are issues that can't be auto-fixed, the commit will be aborted with instructions.

**Note:** Unlike the old hooks, prek does not auto-stage fixed files. If files are modified by the hooks, you'll need to `git add` them and commit again.

## Bypassing the hooks

Sometimes you need to commit work-in-progress or have a valid reason to skip the checks:

### Option 1: Use --no-verify flag

```bash
git commit --no-verify -m "WIP: experimental changes"
```

### Option 2: Skip specific hooks

```bash
SKIP=cargo-clippy git commit -m "message"
# or
PREK_SKIP=cargo-clippy git commit -m "message"

Supports comma-separated values for multiple hooks:
SKIP=cargo-clippy,cargo-fmt git commit -m "message"
```

## Running hooks manually

```bash
# Run all hooks on all files
prek run --all-files

# Run a specific hook
prek run cargo-fmt --all-files
```

## Manual fixes

The hook automatically fixes most issues, but if it fails on clippy warnings that can't be auto-fixed:

### For remaining clippy warnings

Review the clippy output and fix the warnings manually. Common issues that need manual fixes:

- Logic errors or potential bugs
- Performance issues that require refactoring
- Missing documentation on public items
- Complex type inference issues
- Unsafe code that needs review

## Troubleshooting

If the hook isn't running:

1. Make sure you've run `prek install`
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
prek install
```
