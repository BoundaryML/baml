# BEPs Management Scripts

Python scripts for managing BAML Enhancement Proposals (BEPs).

## Available Scripts

### Create New BEP

```bash
mise run bep:new
# Or: ./beps/scripts/new_bep.py
```

Interactively creates a new BEP:
- Generates next BEP number
- Creates directory structure
- Adds template README.md
- Initializes frontmatter (title, status, authors, etc.)

### Update BEP Metadata

```bash
mise run bep:update BEP-001
# Or: ./beps/scripts/update_bep.py BEP-001
```

Updates BEP metadata:
- Change status (draft, accepted, rejected, etc.)
- Update authors
- Modify dates
- Edit other frontmatter fields

### Update BEPs README

```bash
mise run bep:readme
# Or: ./beps/scripts/update_bep_readme.py
```

Regenerates the BEPs table in `beps/docs/README.md`:
- Scans all BEP directories
- Extracts metadata from frontmatter
- Updates the summary table
- Runs automatically via git pre-commit hook

Use `--check` flag to verify without modifying:
```bash
./beps/scripts/update_bep_readme.py --check
```

## Mise Integration

All scripts are integrated with [mise](https://mise.jdx.dev/) tasks:

```bash
# View all BEP tasks
mise tasks | grep bep

# Available tasks:
mise run bep:new      # Create new BEP
mise run bep:update   # Update BEP metadata
mise run bep:readme   # Update README table
mise run bep:check    # Check README is up to date
mise run bep:serve    # Serve docs locally
```

## Git Hooks

The repository includes a pre-commit hook that automatically runs `bep:readme` to keep the summary table up to date.

Setup hooks:
```bash
./scripts/setup-hooks.sh
```

## Requirements

- Python 3.8+
- No additional dependencies (uses stdlib only)

## Infrastructure Deployment

For AWS infrastructure setup and deployment, see:
- [../infrastructure/](../infrastructure/) - AWS CDK infrastructure
- [../DEPLOYMENT.md](../DEPLOYMENT.md) - Deployment guide

