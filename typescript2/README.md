# BAML Playground v2

VSCode extension and web app for the BAML Playground.

## Project Structure

```
typescript2/
├── app-vscode-ext/      # VSCode extension (hosts the webview)
├── app-vscode-webview/  # Vite React app (rendered in VSCode webview)
├── app-promptfiddle/    # Standalone web app
└── pkg-playground/      # Shared playground logic
```

## Development

```bash
# Install dependencies
pnpm install

# Build all packages
pnpm build && pnpm build:wasm

# Type check all packages
pnpm typecheck
```

### VSCode Extension Development

Run these in separate terminals:

```bash
# Terminal 1: Watch-build the extension
pnpm dev:vscode

# Terminal 2: Run Vite dev server for webview
# Depends on 'pnpm build:wasm' or 'pnpm dev:wasm'
pnpm dev:webview

# Terminal 3 (optional): Watch and rebuild WASM on Rust changes
pnpm dev:wasm
```

Then in VSCode, press `F5` and select **"Launch VS Code extension (v2)"**.

### Building a Local VSIX

From `typescript2/`, build and package the VS Code extension with:

```bash
pnpm vscode:package
```

The VSIX is written to:

```text
app-vscode-ext/app-vscode-ext-0.1.0.vsix
```

What `pnpm vscode:package` does:

1. Removes stale packaged outputs:

   ```bash
   rm -rf app-vscode-webview/dist app-vscode-ext/dist app-vscode-ext/*.vsix
   ```

2. Builds the browser WASM bridge:

   ```bash
   pnpm build:wasm
   ```

   The current VSIX does not bundle this WASM output; it ships the native `baml-cli` and the webview talks to that CLI over WebSocket. Building WASM here makes the local packaging command catch browser/WASM regressions too.

3. Builds the release language-server CLI:

   ```bash
   cargo build -p baml_cli --manifest-path ../baml_language/Cargo.toml --release
   ```

4. Builds the packaged React webview:

   ```bash
   pnpm --filter app-vscode-webview build
   ```

5. Builds the VS Code extension host bundle:

   ```bash
   pnpm --filter app-vscode-ext build
   ```

6. Stages runtime assets into the extension package:

   ```bash
   rm -rf app-vscode-ext/dist/playground app-vscode-ext/dist/baml-cli
   mkdir -p app-vscode-ext/dist/playground app-vscode-ext/dist/baml-cli
   cp -R app-vscode-webview/dist/. app-vscode-ext/dist/playground/
   cp ../baml_language/target/release/baml-cli app-vscode-ext/dist/baml-cli/baml-cli
   chmod 755 app-vscode-ext/dist/baml-cli/baml-cli
   ```

7. Packages the VSIX:

   ```bash
   cd app-vscode-ext
   pnpm dlx @vscode/vsce package --no-dependencies --allow-missing-repository --skip-license
   ```

The packaged extension includes `dist/extension.js`, the built webview under `dist/playground`, and the bundled `baml-cli` under `dist/baml-cli`. The bundled CLI is platform-specific, so build the VSIX on the same platform/architecture as the machine that will install it.

### Standalone Web App

```bash
pnpm dev:promptfiddle
```

## Testing Instructions

### VSCode Extension Tests

The VSCode extension uses [Vitest](https://vitest.dev/) for unit testing.

```bash
pnpm --filter app-vscode-ext test      # watch mode
pnpm --filter app-vscode-ext test:run  # single run
```

Tests are located in `app-vscode-ext/src/**/__tests__/`.

### Webview Tests

The webview app (`app-vscode-webview`) uses Vitest with [React Testing Library](https://testing-library.com/docs/react-testing-library/intro/) for component testing.

```bash
pnpm --filter app-vscode-webview test:browser      # watch mode
pnpm --filter app-vscode-webview test:browser:run  # single run
pnpm --filter app-vscode-webview test:unit
pnpm --filter app-vscode-webview test:unit:run
```

- Use browser tests (`*.browser.test.ts,tsx`) by default: these allow testing components that depend on WASM.
  - the `wasm-bindgen` shim that we need to `initWasm()` requires a browser-based implementation of `fetch`, so testing anything that depends on WASM must go in a browser test
  - Browser tests run against Chromium via Playwright.
- Use unit tests (`*.test.ts,tsx` excluding the above) if you don't need to depend on WASM or other browser APIs. These use `@testing-library/react` which is backed by `jsdom`, a fake browser implementation.

### Running All Tests

```bash
# From the typescript2 directory
pnpm --filter app-vscode-ext test:run
pnpm --filter app-vscode-webview test:run  # Runs unit and browser tests
```
