import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { ToolingBackend } from '../src/backend.js';
import { DiskArtifactCache } from '../src/cache.js';
import {
  buildLineIndex,
  utf8OffsetToUtf16,
  utf16OffsetToUtf8,
} from '../src/encoding.js';
import {
  type ToolingResponse as Response,
  ToolingRequest,
  ToolingResponse,
} from '../src/generated/tooling.js';
import { generatedToSource, sourceToGenerated } from '../src/mapping.js';
import { BamlProject } from '../src/project.js';
import { isBamlSidecar, sidecarFingerprint } from '../src/sidecar.js';

describe('offset conversion', () => {
  it('round trips astral Unicode and CRLF', () => {
    const index = buildLineIndex('a😀\r\nb');
    for (const utf16 of [0, 1, 3, 4, 5, 6]) {
      const utf8 = utf16OffsetToUtf8(index, utf16);
      expect(utf8OffsetToUtf16(index, utf8)).toBe(utf16);
    }
  });
});

describe('segment maps', () => {
  const map = {
    generatedFile: 'virtual.d.ts',
    segments: [
      {
        genLengthUtf16: 6,
        genStartUtf16: 7,
        role: 'declaration',
        signatureId: '',
        sourceFile: 0,
        sourceLengthUtf8: 6,
        sourceStartUtf8: 10,
        symbolId: 'T:user.Person',
      },
    ],
    sourceHashes: ['abc'],
    sources: ['/p/main.baml'],
    version: 1,
  };

  it('translates in both directions by compiler symbol identity', () => {
    expect(generatedToSource(map, 9)?.symbolId).toBe('T:user.Person');
    expect(sourceToGenerated(map, '/p/main.baml', 12)).toHaveLength(1);
    expect(generatedToSource(map, 0)).toBeUndefined();
  });
});

describe('sidecars', () => {
  it('uses one predicate for both override forms', () => {
    expect(isBamlSidecar('x.baml.ts')).toBe(true);
    expect(isBamlSidecar('x.baml.d.ts')).toBe(true);
    expect(isBamlSidecar('x.baml')).toBe(false);
    expect(sidecarFingerprint('// baml-fingerprint: abc123\n')).toBe('abc123');
  });
});

describe('disk cache', () => {
  it('recovers from corruption and single-flights misses', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'baml-cache-'));
    const cache = new DiskArtifactCache(directory);
    const key = cache.key(['compiler', new Uint8Array([1, 2])]);
    await cache.put(key, { ok: true });
    expect(await cache.get(key)).toEqual({ ok: true });
    await writeFile(cache.path(key), 'partial');
    expect(await cache.get(key)).toBeUndefined();
    let calls = 0;
    const produce = () =>
      cache.singleFlight(key, async () => {
        calls++;
        return 42;
      });
    expect(await Promise.all([produce(), produce()])).toEqual([42, 42]);
    expect(calls).toBe(1);
    expect(await readFile(cache.path(key), 'utf8')).toBe('partial');
  });
});

class ContractBackend implements ToolingBackend {
  moduleCalls = 0;
  text = '';
  path = '';
  version = 1;
  constructor(readonly kind: 'native' | 'wasm') {}
  dispatch(bytes: Uint8Array): Uint8Array {
    const request = ToolingRequest.decode(bytes).request;
    let response: Response['response'];
    switch (request?.$case) {
      case 'open':
        this.path = request.open.files[0]?.path ?? '';
        this.text = request.open.files[0]?.text ?? '';
        response = {
          $case: 'project',
          project: {
            fingerprint: this.fingerprint(),
            projectId: 'p1',
            revision: this.version,
          },
        };
        break;
      case 'layout':
        response = {
          $case: 'layout',
          layout: {
            configPath: join(
              request.layout.projectId === 'p1' ? this.path : '',
              '../baml.toml',
            ),
            roots: [join(this.path, '..')],
            sourceFiles: [this.path],
            watchFiles: [this.path],
          },
        };
        break;
      case 'capabilities':
        response = {
          $case: 'capabilities',
          capabilities: {
            compilerVersion: 'test',
            features: ['typescriptImports.v1', 'rename.v1'],
            protocol: 'baml.tooling.v1',
          },
        };
        break;
      case 'check':
        response = {
          $case: 'check',
          check: { diagnostics: [], revision: this.version },
        };
        break;
      case 'update':
        this.text = request.update.file?.text ?? '';
        this.version++;
        response = {
          $case: 'project',
          project: {
            fingerprint: this.fingerprint(),
            projectId: 'p1',
            revision: this.version,
          },
        };
        break;
      case 'module':
        this.moduleCalls++;
        response = {
          $case: 'module',
          module: {
            code: 'export const b = {}',
            declaration: `// baml-fingerprint: ${this.fingerprint()}\nexport declare const b: {};\n`,
            fingerprint: this.fingerprint(),
            id: request.module.specifier,
            map: {
              generatedFile: request.module.specifier,
              segments: [],
              sourceHashes: [this.fingerprint()],
              sources: [this.path],
              version: 1,
            },
            revision: this.version,
            runtimeId: '\\0baml:p1:runtime',
            stale: false,
            watchFiles: [this.path],
          },
        };
        break;
      default:
        response = {
          $case: 'error',
          error: { code: 'unsupported', message: request?.$case ?? 'empty' },
        };
    }
    return ToolingResponse.encode({ response }).finish();
  }
  fingerprint() {
    return createHash('sha256').update(this.text).digest('hex');
  }
}

describe.each(['native', 'wasm'] as const)(
  '%s compiler bridge contract',
  (kind) => {
    it('opens, checks, emits deterministically, and applies an overlay', async () => {
      const root = await mkdtemp(join(tmpdir(), `baml-${kind}-`));
      const source = join(root, 'baml_src/main.baml');
      await mkdir(join(root, 'baml_src'));
      await writeFile(join(root, 'baml.toml'), '');
      await writeFile(source, 'class Person {}\n');
      const backend = new ContractBackend(kind);
      const project = await BamlProject.open({ backend, cwd: root });
      expect(project.check().diagnostics).toEqual([]);
      expect(project.resolveDts(source, source).code).toContain(
        'export declare const b',
      );
      const before = project.fingerprint();
      project.updateFile(source, 'class Human {}\n');
      expect(project.fingerprint()).not.toBe(before);
      expect(project.resolveDts(source, source).stale).toBe(false);
    });
  },
);

describe('project artifact cache', () => {
  it('hydrates hash-matching modules and misses after source changes', async () => {
    const root = await mkdtemp(join(tmpdir(), 'baml-project-cache-'));
    const cacheDir = join(root, 'cache');
    const source = join(root, 'main.baml');
    await writeFile(join(root, 'baml.toml'), '');
    await writeFile(source, 'class Person {}\n');

    const firstBackend = new ContractBackend('native');
    const first = await BamlProject.open({
      backend: firstBackend,
      cacheDir,
      cwd: root,
    });
    first.resolveDts(source, source);
    expect(firstBackend.moduleCalls).toBe(1);
    await new Promise((resolveWait) => setTimeout(resolveWait, 25));

    const hitBackend = new ContractBackend('native');
    const hit = await BamlProject.open({
      backend: hitBackend,
      cacheDir,
      cwd: root,
    });
    hit.resolveDts(source, source);
    expect(hitBackend.moduleCalls).toBe(0);

    await writeFile(source, 'class Human {}\n');
    const missBackend = new ContractBackend('native');
    const miss = await BamlProject.open({
      backend: missBackend,
      cacheDir,
      cwd: root,
    });
    miss.resolveDts(source, source);
    expect(missBackend.moduleCalls).toBe(1);
  });
});
