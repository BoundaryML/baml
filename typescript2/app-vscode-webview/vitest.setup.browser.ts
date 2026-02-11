import '@testing-library/jest-dom/vitest';
import { beforeAll } from 'vitest';

// Initialize WASM before tests run
// In browser mode, fetch works natively so no patching needed
beforeAll(async () => {
  const initWasm = (await import('@b/bridge_wasm')).default;
  await initWasm();
});
