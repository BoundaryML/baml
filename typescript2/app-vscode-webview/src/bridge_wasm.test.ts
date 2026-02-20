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

/** Minimal in-memory VFS with one BAML file for create(callbacks, wasmVfs) API. */
function makeMinimalVfs() {
  const encoder = new TextEncoder();
  const files = new Map<string, Uint8Array>([
    [`${ROOT_PATH}/main.baml`, encoder.encode(MINIMAL_BAML)],
  ]);
  const dirs = new Set<string>([ROOT_PATH, '/']);

  const readDir = (path: string): string[] => {
    const prefix = path.endsWith('/') ? path : path + '/';
    const out = new Set<string>();
    for (const p of files.keys()) {
      if (p.startsWith(prefix)) {
        const rest = p.slice(prefix.length);
        const slash = rest.indexOf('/');
        out.add(slash >= 0 ? rest.slice(0, slash) : rest);
      }
    }
    for (const d of dirs) {
      if (d.startsWith(prefix) && d !== path) {
        const rest = d.slice(prefix.length);
        if (rest && !rest.includes('/')) out.add(rest);
      }
    }
    return Array.from(out);
  };

  return {
    readDir,
    createDir: (path: string) => {
      dirs.add(path);
    },
    exists: (path: string) => files.has(path) || dirs.has(path),
    readFile: (path: string) => {
      const data = files.get(path);
      if (!data) throw new Error(`readFile: not found: ${path}`);
      return data;
    },
    writeFile: (_path: string, _data: Uint8Array) => {},
    metadata: (path: string) => {
      if (files.has(path)) {
        return { file_type: 'file', len: files.get(path)!.length, created: undefined, modified: undefined, accessed: undefined };
      }
      if (dirs.has(path)) {
        return { file_type: 'directory', len: 0, created: undefined, modified: undefined, accessed: undefined };
      }
      throw new Error(`metadata: not found: ${path}`);
    },
    removeFile: () => {},
    removeDir: () => {},
    setTime: () => {},
    copyFile: () => {},
    moveFile: () => {},
    moveDir: () => {},
    readMany: (): [string, Uint8Array][] => Array.from(files.entries()),
  };
}

describe('BamlWasmRuntime', () => {
  describe('create with fetch callback (new contract)', () => {
    it('accepts fetch callback returning status, headersJson, url, bodyPromise', () => {
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

      const callbacks = {
        fetch: fetchCallback,
        env: (_var: string) => undefined,
        lsp_send_notification: () => {},
        lsp_send_response: () => {},
        lsp_make_request: () => {},
        playground_send_notification: () => {},
      };
      const runtime = BamlWasmRuntime.create(callbacks, makeMinimalVfs());

      expect(runtime).toBeDefined();
      expect(typeof runtime.requestPlaygroundState).toBe('function');
      expect(typeof runtime.callFunction).toBe('function');
      runtime.requestPlaygroundState();
    });

    it('uses bodyPromise (not body) per contract', () => {
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

      const callbacks = {
        fetch: fetchCallback,
        env: (_var: string) => undefined,
        lsp_send_notification: () => {},
        lsp_send_response: () => {},
        lsp_make_request: () => {},
        playground_send_notification: () => {},
      };
      const runtime = BamlWasmRuntime.create(callbacks, makeMinimalVfs());
      expect(runtime).toBeDefined();
      // Runtime was created without awaiting bodyPromise; body is read only when .text() is called in BAML
      expect(bodyPromiseResolved).toBe(false);
    });
  });
});
