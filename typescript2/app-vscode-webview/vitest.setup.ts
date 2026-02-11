import '@testing-library/jest-dom/vitest';
import { vi, beforeAll } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = dirname(fileURLToPath(import.meta.url));
const wasmDir = resolve(projectRoot, '../pkg-playground/wasm');

// Patch global fetch to handle WASM file loading in Node.js environment
const originalFetch = globalThis.fetch;
globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
  const url = input instanceof Request ? input.url : input.toString();

  // Handle file:// URLs
  if (url.startsWith('file://')) {
    const filePath = fileURLToPath(url);
    const buffer = readFileSync(filePath);
    return new Response(buffer, {
      status: 200,
      headers: { 'Content-Type': 'application/wasm' },
    });
  }

  // Handle .wasm file requests (Vite transforms import.meta.url to http URLs)
  if (url.endsWith('.wasm')) {
    const wasmPath = resolve(wasmDir, 'bridge_wasm_bg.wasm');
    const buffer = readFileSync(wasmPath);
    return new Response(buffer, {
      status: 200,
      headers: { 'Content-Type': 'application/wasm' },
    });
  }

  // Fall back to original fetch for other URLs
  return originalFetch(input, init);
};

// Initialize WASM before tests run
beforeAll(async () => {
  const initWasm = (await import('@b/bridge_wasm')).default;
  await initWasm();
});

// Mock jotai-devtools to avoid CSS import issues in tests
vi.mock('jotai-devtools', () => ({
  DevTools: () => null,
}));
