import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  ToolingRequest,
  ToolingResponse,
} from '../../pkg-baml-tooling/src/generated/tooling.js';

const repository = resolve(import.meta.dirname, '../../..');

describe('native and wasm tooling hosts', () => {
  it('return byte-identical responses for the same request corpus', async () => {
    const require = createRequire(import.meta.url);
    const nativeModule = require(
      resolve(
        repository,
        'baml_language/sdks/typescript/bridge_tooling/baml-tooling-node.js',
      ),
    ) as {
      BamlToolingBridge: new () => {
        dispatch(bytes: Uint8Array): Uint8Array;
      };
    };
    const wasmDirectory = resolve(
      repository,
      'typescript2/pkg-playground/wasm',
    );
    const wasmModule = (await import(
      resolve(wasmDirectory, 'bridge_wasm.js')
    )) as {
      default(input: { module_or_path: Uint8Array }): Promise<unknown>;
      BamlToolingBridge: new () => {
        dispatch(bytes: Uint8Array): Uint8Array;
      };
    };
    await wasmModule.default({
      module_or_path: await readFile(
        resolve(wasmDirectory, 'bridge_wasm_bg.wasm'),
      ),
    });

    const native = new nativeModule.BamlToolingBridge();
    const wasm = new wasmModule.BamlToolingBridge();
    const dispatchPair = (request: ToolingRequest) => {
      const encoded = ToolingRequest.encode(request).finish();
      const nativeBytes = new Uint8Array(native.dispatch(encoded));
      const wasmBytes = new Uint8Array(wasm.dispatch(encoded));
      expect(wasmBytes).toEqual(nativeBytes);
      return ToolingResponse.decode(nativeBytes);
    };

    const root = resolve(repository, 'typescript2/pkg-ts-tooling-e2e/.parity');
    const path = resolve(root, 'main.baml');
    const configPath = resolve(root, 'baml.toml');
    const config = '[package]\nname = "parity"\n';
    const source =
      '/// Bag docs\nclass Bag { scores map<string, int> }\nfunction Read(input: Bag) -> string { "ok" }\n';
    const opened = dispatchPair({
      request: {
        $case: 'open',
        open: {
          files: [
            { path, text: source },
            { path: configPath, text: config },
          ],
          projectRoot: root,
          target: 'node',
        },
      },
    });
    if (opened.response?.$case !== 'project')
      throw new Error('open did not return project state');
    const projectId = opened.response.project.projectId;
    const bagOffset = Buffer.byteLength(
      source.slice(0, source.indexOf('Bag {')),
    );
    const requests: ToolingRequest[] = [
      { request: { $case: 'check', check: { projectId } } },
      { request: { $case: 'layout', layout: { projectId } } },
      { request: { $case: 'capabilities', capabilities: { projectId } } },
      {
        request: {
          $case: 'module',
          module: { importer: path, projectId, specifier: path },
        },
      },
      {
        request: {
          $case: 'definition',
          definition: { offsetUtf8: bagOffset, path, projectId, symbolId: '' },
        },
      },
      {
        request: {
          $case: 'references',
          references: { offsetUtf8: bagOffset, path, projectId, symbolId: '' },
        },
      },
      {
        request: {
          $case: 'hover',
          hover: { offsetUtf8: 0, path: '', projectId, symbolId: 'T:user.Bag' },
        },
      },
      {
        request: {
          $case: 'completions',
          completions: { offsetUtf8: 0, path: '', projectId, symbolId: '' },
        },
      },
      {
        request: {
          $case: 'prepareRename',
          prepareRename: {
            offsetUtf8: 0,
            path: '',
            projectId,
            symbolId: 'T:user.Bag',
          },
        },
      },
      {
        request: {
          $case: 'rename',
          rename: {
            newName: 'Container',
            offsetUtf8: 0,
            path: '',
            projectId,
            symbolId: 'T:user.Bag',
          },
        },
      },
      { request: { $case: 'runtimeModule', runtimeModule: { projectId } } },
    ];
    for (const request of requests) dispatchPair(request);

    // Disposal is part of the protocol, not a host-side no-op. Both hosts
    // must release the session identically — byte equality here is what stops
    // native and WASM drifting into one freeing what the other leaks.
    const closed = dispatchPair({
      request: { $case: 'close', close: { projectId } },
    });
    expect(closed.response?.$case).toBe('closed');
    if (closed.response?.$case === 'closed')
      expect(closed.response.closed.released).toBe(true);

    // The released id is dead on both hosts: a closed session is gone, not
    // merely unreachable.
    expect(
      dispatchPair({ request: { $case: 'check', check: { projectId } } })
        .response?.$case,
    ).toBe('error');

    // Closing twice is idempotent on both hosts.
    const again = dispatchPair({
      request: { $case: 'close', close: { projectId } },
    });
    if (again.response?.$case === 'closed')
      expect(again.response.closed.released).toBe(false);
  }, 240_000);

  it('isolates same-root sessions per bridge instance on both backends', async () => {
    const require = createRequire(import.meta.url);
    const nativeModule = require(
      resolve(
        repository,
        'baml_language/sdks/typescript/bridge_tooling/baml-tooling-node.js',
      ),
    ) as {
      BamlToolingBridge: new () => {
        dispatch(bytes: Uint8Array): Uint8Array;
      };
    };
    const wasmDirectory = resolve(
      repository,
      'typescript2/pkg-playground/wasm',
    );
    const wasmModule = (await import(
      resolve(wasmDirectory, 'bridge_wasm.js')
    )) as {
      default(input: { module_or_path: Uint8Array }): Promise<unknown>;
      BamlToolingBridge: new () => {
        dispatch(bytes: Uint8Array): Uint8Array;
      };
    };
    await wasmModule.default({
      module_or_path: await readFile(
        resolve(wasmDirectory, 'bridge_wasm_bg.wasm'),
      ),
    });

    // Two clients serving the same root (client + SSR environments, two
    // projects in one test process) must never observe each other's
    // overlays. Native and WASM backends guarantee the same isolation.
    for (const bridge of [
      () => new nativeModule.BamlToolingBridge(),
      () => new wasmModule.BamlToolingBridge(),
    ]) {
      const first = bridge();
      const second = bridge();
      const root = resolve(
        repository,
        'typescript2/pkg-ts-tooling-e2e/.parity',
      );
      const path = resolve(root, 'main.baml');
      const open = (bridgeInstance: {
        dispatch(bytes: Uint8Array): Uint8Array;
      }) => {
        const response = ToolingResponse.decode(
          bridgeInstance.dispatch(
            ToolingRequest.encode({
              request: {
                $case: 'open',
                open: {
                  files: [{ path, text: 'class One {}\n' }],
                  projectRoot: root,
                  target: 'node',
                },
              },
            }).finish(),
          ),
        );
        if (response.response?.$case !== 'project')
          throw new Error('open did not return project state');
        return response.response.project.projectId;
      };
      const declaration = (
        bridgeInstance: { dispatch(bytes: Uint8Array): Uint8Array },
        projectId: string,
      ) => {
        const response = ToolingResponse.decode(
          bridgeInstance.dispatch(
            ToolingRequest.encode({
              request: {
                $case: 'module',
                module: { importer: path, projectId, specifier: path },
              },
            }).finish(),
          ),
        );
        if (response.response?.$case !== 'module')
          throw new Error(`module request failed: ${JSON.stringify(response)}`);
        return response.response.module.declaration;
      };
      const firstId = open(first);
      const secondId = open(second);
      first.dispatch(
        ToolingRequest.encode({
          request: {
            $case: 'update',
            update: {
              file: { path, text: 'class Two {}\n' },
              projectId: firstId,
              remove: false,
              version: 1,
            },
          },
        }).finish(),
      );
      expect(declaration(first, firstId)).toContain('interface Two');
      expect(declaration(second, secondId)).toContain('interface One');
    }
  }, 30_000);
});
