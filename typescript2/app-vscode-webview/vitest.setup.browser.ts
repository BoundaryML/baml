import '@testing-library/jest-dom/vitest';
import { beforeAll } from 'vitest';

// Initialize WASM before tests run
// In browser mode, fetch works natively so no patching needed
beforeAll(async () => {
  const initWasm = (await import('@b/baml-playground-wasm')).default;
  await initWasm();
});
