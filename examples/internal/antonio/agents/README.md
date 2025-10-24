# stock-agent

Python BAML workflow

## Setup

```bash
# Set your Boundary API key in .env
echo BOUNDARY_API_KEY=sk-key-YOUR_KEY > .env

# Allow direnv (if using)
direnv allow

# Set up everything (install deps, build BAML, generate)
pnpm setup

# Run the workflow
pnpm dev
```

## Available Scripts

- `pnpm dev` - Run the Python workflow
- `pnpm setup` - One-liner setup (sync + build:baml + generate)
- `pnpm sync` - Install/update Python dependencies
- `pnpm build:baml` - Build BAML Python client
- `pnpm generate` - Generate BAML client code
- `pnpm test` - Run tests
- `pnpm typecheck` - Run type checking with mypy
