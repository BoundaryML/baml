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

#### HMR Tests

HMR tests verify that WASM hot reload works:

```bash
pnpm --filter app-vscode-webview test:hmr      # Watch mode
pnpm --filter app-vscode-webview test:hmr:run  # Single run
```

Tests are located in `app-vscode-webview/src/**/*.hmr.test.ts`.

These tests spawn a Vite dev server and verify that changes to Rust source files trigger WASM rebuilds and HMR updates.

### Running All Tests

```bash
# From the typescript2 directory
pnpm --filter app-vscode-ext test:run
pnpm --filter app-vscode-webview test:run  # Runs unit, browser, and hmr tests
```