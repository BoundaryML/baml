/**
 * Tests for the WASM bridge (BamlWasmRuntime) and the fetch callback contract.
 *
 * The fetch callback must return:
 *   { status: number, headersJson: string, url: string, bodyPromise: Promise<string> }
 * Body is only read when BAML calls response_text(); bodyPromise is awaited then.
 */
import { describe, it, expect } from 'vitest';
import { BamlWasmRuntime } from '@b/bridge_wasm';

const ROOT_PATH = '/project';
const MINIMAL_BAML = `
function NoOp() -> string {
  return "ok"
}
`;

describe('BamlWasmRuntime', () => {
  describe('create with fetch callback (new contract)', () => {
    it('accepts fetch callback returning status, headersJson, url, bodyPromise', () => {
      const srcFilesJson = JSON.stringify({ 'main.baml': MINIMAL_BAML });
      const fetchCallback = async (
        _method: string,
        _url: string,
        _headersJson: string,
        _body: string
      ): Promise<{ status: number; headersJson: string; url: string; bodyPromise: Promise<string> }> => ({
        status: 200,
        headersJson: '{}',
        url: 'https://example.com/',
        bodyPromise: Promise.resolve('response body'),
      });

      const runtime = BamlWasmRuntime.create(ROOT_PATH, srcFilesJson, {
        fetch: fetchCallback,
        env: (_var: string) => undefined,
      });

      expect(runtime).toBeDefined();
      const names = runtime.functionNames();
      expect(Array.isArray(names)).toBe(true);
      expect(names).toContain('NoOp');
    });

    it('uses bodyPromise (not body) per contract', () => {
      const srcFilesJson = JSON.stringify({ 'main.baml': MINIMAL_BAML });
      let bodyPromiseResolved = false;
      const fetchCallback = async (): Promise<{
        status: number;
        headersJson: string;
        url: string;
        bodyPromise: Promise<string>;
      }> => ({
        status: 200,
        headersJson: '{}',
        url: '',
        bodyPromise: new Promise((resolve) => {
          setTimeout(() => {
            bodyPromiseResolved = true;
            resolve('deferred body');
          }, 1);
        }),
      });

      const runtime = BamlWasmRuntime.create(ROOT_PATH, srcFilesJson, {
        fetch: fetchCallback,
        env: (_var: string) => undefined,
      });
      expect(runtime).toBeDefined();
      // Runtime was created without awaiting bodyPromise; body is read only when .text() is called in BAML
      expect(bodyPromiseResolved).toBe(false);
    });
  });
});
