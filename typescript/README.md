# BAML TypeScript Monorepo

This directory contains all TypeScript-related packages and applications for BAML, organized as a monorepo managed by Turbo and pnpm.

## Project Structure

After the TypeScript refactor, the codebase is organized as follows:

```
typescript/
├── apps/                    # All applications
│   ├── fiddle-web-app/     # Web playground application
│   ├── playground/         # Playground UI
│   └── vscode-ext/         # VSCode extension
├── packages/               # All reusable packages
│   ├── ui/                # Shared UI components
│   ├── common/            # Common utilities
│   ├── playground-common/ # Playground shared code
│   ├── codemirror-lang-baml/  # CodeMirror language support
│   ├── fiddle-proxy/      # Fiddle proxy server
│   └── nextjs-plugin/     # Next.js integration
└── workspace-tools/       # Build and config tools
```

## Development

### Prerequisites

- Node.js (LTS version)
- pnpm 9.12.0 (managed via mise)
- Rust toolchain (for building WASM modules)

### Setup

1. From the root directory, run the setup script:
```bash
./scripts/setup-dev.sh
```

2. Install dependencies:
```bash
pnpm install
```

3. Build all packages:
```bash
pnpm build
```

### Development Commands

From the root directory:

- `pnpm dev` - Start all development servers
- `pnpm dev:vscode` - Develop VSCode extension
- `pnpm dev:playground` - Develop playground
- `pnpm dev:language-server` - Develop language server
- `pnpm build` - Build all packages
- `pnpm build:fiddle-web-app` - Build web app and dependencies
- `pnpm build:vscode` - Build VSCode extension and dependencies
- `pnpm clean:ws` - Clean all build artifacts
- `pnpm typecheck` - Run TypeScript type checking

### VSCode Extension Development

The VSCode extension is located in `typescript/apps/vscode-ext/`. To develop:

1. Navigate to the TypeScript directory: `cd typescript/`
2. Install dependencies: `pnpm i`
3. Build and launch: `pnpm build:vscode`
4. Open VSCode and use the Run and Debug view
5. Select "Launch VSCode Extension" and press play

For detailed instructions, see [CONTRIBUTING.md](../CONTRIBUTING.md#vscode-extension-testing).

### Web App Development

The web playground is located in `typescript/apps/fiddle-web-app/`. To develop:

1. Navigate to the app: `cd typescript/apps/fiddle-web-app`
2. Start the dev server: `pnpm dev`
3. The app will hot-reload when you modify files

### Package Development

All shared packages are in `typescript/packages/`. When developing packages:

1. Make changes in the package directory
2. Run `pnpm build` in the package directory
3. Packages that depend on it will automatically pick up changes

## Testing

- Run all tests: `pnpm test`
- Run specific package tests: Navigate to the package and run `pnpm test`

## Troubleshooting

- If you encounter build errors, try: `pnpm clean:ws && pnpm install`
- For WASM-related issues, ensure Rust toolchain is installed
- For dependency issues, check that you're using the correct pnpm version

For more detailed development instructions, see the main [CONTRIBUTING.md](../CONTRIBUTING.md) guide.
