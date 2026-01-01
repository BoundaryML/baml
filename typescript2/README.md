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
pnpm build

# Type check all packages
pnpm typecheck
```

### VSCode Extension Development

Run these in separate terminals:

```bash
# Terminal 1: Watch-build the extension
pnpm dev:ext

# Terminal 2: Run Vite dev server for webview (serves on port 4000)
pnpm dev:webview
```

Then in VSCode, press `F5` and select **"Launch VS Code extension (v2)"**.

### Standalone Web App

```bash
pnpm dev:web
```

## Testing Instructions

### VSCode Extension Tests

The VSCode extension uses [Vitest](https://vitest.dev/) for unit testing.

```bash
# Run tests once
pnpm --filter app-vscode-ext test

# Run tests in watch mode
pnpm --filter app-vscode-ext test:watch
```

Tests are located in `app-vscode-ext/src/**/__tests__/`.

### Webview Tests

The webview app (`app-vscode-webview`) uses Vitest with [React Testing Library](https://testing-library.com/docs/react-testing-library/intro/) for component testing.

```bash
# Run all tests once (jsdom + browser)
pnpm --filter app-vscode-webview test:run

# Run all tests in watch mode
pnpm --filter app-vscode-webview test

# Run tests with UI
pnpm --filter app-vscode-webview test:ui
```

#### Unit Tests (jsdom)

Unit tests run in jsdom and don't require Playwright:

```bash
pnpm --filter app-vscode-webview test:unit:run  # Single run
pnpm --filter app-vscode-webview test:unit      # Watch mode
```

Tests are located in `app-vscode-webview/src/**/*.test.tsx`.

The setup file (`vitest.setup.ts`) patches `fetch` to handle WASM file loading in the Node.js/jsdom environment.

#### Browser Tests (Playwright)

Browser tests run in a real Chromium browser via Playwright:

```bash
pnpm --filter app-vscode-webview test:browser:run  # Single run
pnpm --filter app-vscode-webview test:browser      # Watch mode
```

Tests are located in `app-vscode-webview/src/**/*.browser.test.tsx`.

These tests use Vitest's browser mode with native `fetch` and full WASM support.

### Running All Tests

```bash
# From the typescript2 directory
pnpm --filter app-vscode-ext test
pnpm --filter app-vscode-webview test:run
```

### Adding New Tests

#### Extension Tests

1. Create a `__tests__` directory next to the code you want to test
2. Create a file named `*.test.ts`
3. Use Vitest's `describe`, `it`, and `expect`:

```typescript
import { describe, it, expect } from 'vitest';
import { myFunction } from '../myFunction';

describe('myFunction', () => {
  it('does something', () => {
    expect(myFunction()).toBe(expected);
  });
});
```

#### Webview Component Tests

1. Create a file named `*.test.tsx` next to the component
2. Use React Testing Library with Vitest:

```typescript
import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MyComponent } from './MyComponent';

describe('MyComponent', () => {
  it('renders correctly', () => {
    render(<MyComponent />);
    expect(screen.getByText('Hello')).toBeInTheDocument();
  });
});
```
