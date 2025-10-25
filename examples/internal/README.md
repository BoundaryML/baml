# BAML Internal Workflow Examples

This directory contains internal BAML workflow examples and templates for quickly setting up new workflows.

## Quick Start

### Every time you make a new workflow

```bash
$ cd $BAML_ROOT/examples/internal/$YOUR_NAME
$ pnpm python-init "YOUR_WORKFLOW_NAME" # will be your folder name
# OR
$ pnpm typescript-init "YOUR_WORKFLOW_NAME" # will be your folder name
$ cd YOUR_WORKFLOW_NAME
$ # Set up a new boundary studio project
$ echo BOUNDARY_API_KEY=sk-key-1241 > .env
$ direnv allow

# For Python workflows:
$ pnpm setup  # One-liner to set up everything
$ pnpm dev

# For TypeScript workflows (already set up):
$ pnpm dev
```

## Available Commands

- `pnpm python-init <workflow_name>` - Create a new Python-based BAML workflow
- `pnpm typescript-init <workflow_name>` - Create a new TypeScript/Next.js-based BAML workflow

## Templates

The `_template` directory contains starter templates for different types of workflows:

- **python-demo**: Python workflow template with BAML integration
- **typescript-demo**: TypeScript/Next.js workflow template with BAML integration
- **mixed-demo**: Combined Python backend and TypeScript frontend template

## Directory Structure

Each person has their own folder to organize their workflows:

- `aaron/`
- `antonio/`
- `greg/`
- `sam/`
- `vaibhav/`

## Python Workflows

For Python workflows, after initialization:

1. Set your Boundary API key in `.env`
2. Run `direnv allow` if using direnv
3. Run `pnpm setup` to install dependencies and set up BAML
4. Run your workflow with `pnpm dev`

Available pnpm scripts for Python workflows:
- `pnpm dev` - Run the Python workflow
- `pnpm build:baml` - Build BAML Python client
- `pnpm generate` - Generate BAML client code
- `uv ..` - this is still available to run uv commands directly!

## TypeScript Workflows

For TypeScript workflows, after initialization (dependencies and BAML are set up automatically):

1. Set your Boundary API key in `.env`
2. Run `direnv allow` if using direnv
3. Run your workflow with `pnpm dev` (runs TypeScript script)
   - Or start the web server with `pnpm web` and open [http://localhost:3000](http://localhost:3000)

Available pnpm scripts for TypeScript workflows:
- `pnpm dev` - Run the TypeScript script (app/script.ts)
- `pnpm web` - Start Next.js development server
- `pnpm generate` - Generate BAML client code
- `pnpm build:baml` - Build BAML TypeScript client

## Notes

- The scripts automatically detect if you're in your personal folder and will create the workflow there
- If you run the init scripts from the root `internal/` directory, you'll be prompted to choose a folder
- The `.env` file is created with a placeholder API key that you need to replace
- Templates exclude generated files like `.venv`, `node_modules`, `baml_client`, etc.
