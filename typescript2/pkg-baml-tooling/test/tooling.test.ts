import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import {
  mkdir,
  mkdtemp,
  readdir,
  readFile,
  realpath,
  rm,
  symlink,
  utimes,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { ToolingBackend } from '../src/backend.js';
import { DiskArtifactCache } from '../src/cache.js';
import {
  buildLineIndex,
  LineIndexCache,
  utf8OffsetToUtf16,
  utf16OffsetToUtf8,
} from '../src/encoding.js';
import {
  type ToolingResponse as Response,
  ToolingRequest,
  ToolingResponse,
} from '../src/generated/tooling.js';
import { generatedToSource, sourceToGenerated } from '../src/mapping.js';
import {
  BamlFileDiscovery,
  BamlProject,
  discoverBamlFiles,
  projectId,
  resolveBamlSpecifier,
} from '../src/project.js';
import { isBamlSidecar, sidecarFingerprint } from '../src/sidecar.js';

describe('offset conversion', () => {
  it('round trips astral Unicode and CRLF', () => {
    const index = buildLineIndex('a😀\r\nb');
    for (const utf16 of [0, 1, 3, 4, 5, 6]) {
      const utf8 = utf16OffsetToUtf8(index, utf16);
      expect(utf8OffsetToUtf16(index, utf8)).toBe(utf16);
    }
  });

  it('caches one index per path and rebuilds only on text change', () => {
    const cache = new LineIndexCache();
    const first = cache.forText('/p/a.baml', 'class A {}\n');
    expect(cache.forText('/p/a.baml', 'class A {}\n')).toBe(first);
    expect(cache.forText('/p/b.baml', 'class A {}\n')).not.toBe(first);
    const rebuilt = cache.forText('/p/a.baml', 'class B {}\n');
    expect(rebuilt).not.toBe(first);
    expect(cache.forText('/p/a.baml', 'class B {}\n')).toBe(rebuilt);
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

  it('evicts oldest entries once the cache grows past its bound', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'baml-cache-bound-'));
    const cache = new DiskArtifactCache(directory, { maxEntries: 4 });
    const keys: string[] = [];
    for (let index = 0; index < 8; index++) {
      const key = cache.key(['edit', String(index)]);
      keys.push(key);
      await cache.put(key, { index });
      // Deterministic oldest-first order regardless of filesystem timestamp
      // granularity.
      const when = new Date(1_700_000_000_000 + index * 1000);
      await utimes(cache.path(key), when, when);
    }
    await cache.prune();
    const entries = (
      await Promise.all(
        (
          await readdir(directory)
        ).map((shard) => readdir(join(directory, shard))),
      )
    ).flat();
    expect(entries.length).toBeLessThanOrEqual(4);
    expect(entries.length).toBeGreaterThan(0);
    // The newest writes survive; the oldest were evicted.
    expect(await cache.get(keys[7] ?? '')).toEqual({ index: 7 });
    expect(await cache.get(keys[0] ?? '')).toBeUndefined();
  });

  it('holds the bound across short-lived processes', async () => {
    // A CLI or build run writes a couple of artifacts and exits, so an
    // in-process write counter never reaches its threshold and never prunes.
    // The cache directory is shared by every run, so the bound has to be
    // derived from what is on disk or it decays into no bound at all.
    const directory = await mkdtemp(join(tmpdir(), 'baml-cache-processes-'));
    const entryCount = async () =>
      (
        await Promise.all(
          (
            await readdir(directory)
          ).map((shard) => readdir(join(directory, shard))),
        )
      ).flat().length;

    const runs = 40;
    const writesPerRun = 2;
    for (let run = 0; run < runs; run++) {
      // Each iteration is one short-lived process: a fresh instance, well
      // under the in-process prune threshold of 32 writes, then exit.
      const cache = new DiskArtifactCache(directory, { maxEntries: 8 });
      for (let write = 0; write < writesPerRun; write++)
        await cache.put(cache.key(['run', String(run), String(write)]), {
          run,
          write,
        });
      // Pruning is amortized, so the directory may sit a few entries over the
      // cap between prunes — but the overshoot is bounded by the writes since
      // the last prune, and never grows with the number of runs.
      expect(await entryCount()).toBeLessThanOrEqual(8 + writesPerRun);
    }

    // 80 writes across 40 processes, none of which ever reaches the
    // in-process threshold, still leave a bounded directory rather than one
    // entry per write.
    expect(await entryCount()).toBeLessThanOrEqual(8 + writesPerRun);
    expect(await entryCount()).toBeLessThan(runs * writesPerRun);
    expect(await entryCount()).toBeGreaterThan(0);

    // The most recent run's artifacts are the ones that survived.
    const last = new DiskArtifactCache(directory, { maxEntries: 8 });
    expect(await last.get(last.key(['run', '39', '1']))).toEqual({
      run: 39,
      write: 1,
    });
  });
});

describe('baml file discovery', () => {
  it('re-walks only when a directory stamp moves', async () => {
    const root = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-discovery-')),
    );
    await writeFile(join(root, 'main.baml'), 'class Main {}\n');
    const discovery = new BamlFileDiscovery(root);

    const first = discovery.poll();
    expect([...(first ?? [])]).toEqual([join(root, 'main.baml')]);
    // No directory changed: the second poll must not re-walk the tree.
    expect(discovery.poll()).toBeUndefined();
    expect(discovery.poll()).toBeUndefined();

    // An added file moves its parent directory's stamp and is discovered.
    await writeFile(join(root, 'added.baml'), 'class Added {}\n');
    const afterAdd = discovery.poll();
    expect([...(afterAdd ?? [])].sort()).toEqual(
      [join(root, 'added.baml'), join(root, 'main.baml')].sort(),
    );
    expect(discovery.poll()).toBeUndefined();

    // A removed file is likewise discovered through the parent stamp.
    await rm(join(root, 'main.baml'));
    const afterRemove = discovery.poll();
    expect([...(afterRemove ?? [])]).toEqual([join(root, 'added.baml')]);
    expect(discovery.poll()).toBeUndefined();
  });

  it('discovers files in newly created subdirectories', async () => {
    const root = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-discovery-sub-')),
    );
    const discovery = new BamlFileDiscovery(root);
    expect(discovery.poll()).toEqual(new Set());
    expect(discovery.poll()).toBeUndefined();
    await mkdir(join(root, 'baml_src'));
    await writeFile(join(root, 'baml_src', 'nested.baml'), 'class N {}\n');
    expect([...(discovery.poll() ?? [])]).toEqual([
      join(root, 'baml_src', 'nested.baml'),
    ]);
  });

  it('discovers a host-only .baml buffer that never reaches the disk', async () => {
    const root = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-discovery-overlay-')),
    );
    const unsaved = join(root, 'unsaved.baml');
    const overlay = new Map([[unsaved, 'class Unsaved {}\n']]);
    // A language-service host that has the buffer open but unsaved: it
    // exists and reads fine through the host, and is invisible to any
    // filesystem walk. Discovery must go through the host abstraction.
    const discovery = new BamlFileDiscovery(root, {
      fileExists: (path) => overlay.has(path) || existsSync(path),
      readFile: (path) => overlay.get(path),
    });

    // Priming sees the empty on-disk tree; the buffer is not yet imported.
    expect(discovery.poll()).toEqual(new Set());
    expect(discovery.poll()).toBeUndefined();

    // The editor creates and imports the buffer. No directory stamp moves,
    // so only the resolved import target can reveal it.
    discovery.track(unsaved);
    expect([...(discovery.poll() ?? [])]).toEqual([unsaved]);
    expect(existsSync(unsaved)).toBe(false);
    // Re-polling is still gated: a known candidate does not force a re-walk.
    expect(discovery.poll()).toBeUndefined();

    // Candidates outside the root, sidecars, and non-BAML paths are ignored.
    discovery.track(join(root, 'unsaved.baml.d.ts'));
    discovery.track(join(root, 'notes.md'));
    discovery.track(join(tmpdir(), 'elsewhere.baml'));
    expect(discovery.poll()).toBeUndefined();
  });

  it('discovers a project whose files exist only inside the host', () => {
    // A WASM/in-memory host: nothing under the root exists on disk at all.
    const root = join(tmpdir(), 'baml-virtual-host-root');
    const files = [join(root, 'main.baml'), join(root, 'src', 'more.baml')];
    const discovery = new BamlFileDiscovery(root, {
      fileExists: (path) => files.includes(path),
      readDirectory: () => [...files, join(root, 'main.baml.d.ts')],
    });
    expect([...(discovery.poll() ?? [])].sort()).toEqual([...files].sort());
    // A root that is absent on both polls has not moved: an in-memory host
    // must not re-walk on every request.
    expect(discovery.poll()).toBeUndefined();
  });

  it('discovers a host-listed buffer created after priming and nothing imports', async () => {
    const root = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-discovery-listed-')),
    );
    await writeFile(join(root, 'main.baml'), 'class Main {}\n');
    const unsaved = join(root, 'unsaved.baml');
    const overlay = new Map<string, string>();
    // The host answers from its own buffers, so the new file is visible
    // through readDirectory/readFile and has no disk entry at all.
    const discovery = new BamlFileDiscovery(root, {
      fileExists: (path) => overlay.has(path) || existsSync(path),
      readDirectory: () => [...overlay.keys()],
      readFile: (path) => overlay.get(path),
    });

    // Priming: the disk tree only. The discovery poll is now armed, so the
    // stamp gate would swallow every later host-only change.
    expect([...(discovery.poll() ?? [])]).toEqual([join(root, 'main.baml')]);
    expect(discovery.poll()).toBeUndefined();

    // The editor creates an unsaved buffer. No disk directory stamp moves
    // (nothing was written), and nothing imports it yet, so `track` is never
    // called: the host listing is the only channel that can reveal it, and
    // it must be consulted before the re-walk gate, not after.
    overlay.set(unsaved, 'class Unsaved {}\n');
    expect([...(discovery.poll() ?? [])].sort()).toEqual(
      [join(root, 'main.baml'), unsaved].sort(),
    );
    expect(existsSync(unsaved)).toBe(false);
    // An unchanged listing still costs no re-walk.
    expect(discovery.poll()).toBeUndefined();

    // Closing the buffer without saving withdraws it from the file set.
    overlay.delete(unsaved);
    expect([...(discovery.poll() ?? [])]).toEqual([join(root, 'main.baml')]);
    expect(discovery.poll()).toBeUndefined();
  });

  it('opens a project from host-listed sources with no disk tree', async () => {
    const root = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-open-overlay-')),
    );
    const unsaved = join(root, 'unsaved.baml');
    const overlay = new Map([[unsaved, 'class Unsaved {}\n']]);
    // discoverBamlFiles is the open-time half of the same abstraction: a
    // host-listed source must join the initial file set even though the
    // directory walk finds nothing.
    const files = discoverBamlFiles(root, {
      readDirectory: () => [unsaved, `${unsaved}.d.ts`],
      readFile: (path) => overlay.get(path),
    });
    expect([...files]).toEqual([[unsaved, 'class Unsaved {}\n']]);
  });
});

describe('baml specifier resolution', () => {
  it('resolves bare specifiers through Node package resolution', async () => {
    const app = await realpath(await mkdtemp(join(tmpdir(), 'baml-resolve-')));
    const dependency = join(app, 'node_modules', 'baml-dep');
    await mkdir(join(dependency, 'baml_src'), { recursive: true });
    await writeFile(
      join(dependency, 'package.json'),
      JSON.stringify({ name: 'baml-dep', version: '1.0.0' }),
    );
    await writeFile(
      join(dependency, 'baml_src', 'widget.baml'),
      'class W {}\n',
    );
    const importer = join(app, 'src', 'index.ts');
    expect(
      resolveBamlSpecifier('baml-dep/baml_src/widget.baml', importer),
    ).toBe(join(dependency, 'baml_src', 'widget.baml'));
    // A package that does not exist resolves to undefined, never to a
    // made-up path under the importer's directory.
    expect(
      resolveBamlSpecifier('missing-dep/baml_src/widget.baml', importer),
    ).toBeUndefined();
  });

  it('canonicalizes pnpm-style symlinks to the physical file', async () => {
    const workspace = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-resolve-pnpm-')),
    );
    const shared = join(workspace, 'packages', 'shared');
    await mkdir(join(shared, 'baml_src'), { recursive: true });
    await writeFile(
      join(shared, 'package.json'),
      JSON.stringify({ name: '@shared/baml', version: '0.0.0' }),
    );
    await writeFile(join(shared, 'baml_src', 's.baml'), 'class S {}\n');
    const modules = join(
      workspace,
      'packages',
      'app',
      'node_modules',
      '@shared',
    );
    await mkdir(modules, { recursive: true });
    await symlink(shared, join(modules, 'baml'), 'dir');
    const importer = join(workspace, 'packages', 'app', 'index.ts');
    expect(resolveBamlSpecifier('@shared/baml/baml_src/s.baml', importer)).toBe(
      join(shared, 'baml_src', 's.baml'),
    );
  });

  it('joins relative specifiers onto the importer directory', async () => {
    const root = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-resolve-rel-')),
    );
    const source = join(root, 'baml_src', 'main.baml');
    await mkdir(join(root, 'baml_src'));
    await writeFile(source, 'class M {}\n');
    expect(
      resolveBamlSpecifier('./baml_src/main.baml', join(root, 'index.ts')),
    ).toBe(source);
    expect(resolveBamlSpecifier(source, join(root, 'index.ts'))).toBe(source);
    expect(resolveBamlSpecifier('baml:client', join(root, 'index.ts'))).toBe(
      'baml:client',
    );
  });
});

class TestBackend implements ToolingBackend {
  readonly kind = 'native' as const;
  moduleCalls = 0;
  moduleSpecifiers: string[] = [];
  openedFiles: string[] = [];
  /** Sessions the bridge is still holding, keyed the way the real protocol
   * keys them: closing removes one, and requests against a released id fail. */
  sessions = new Set<string>();
  closeCalls: string[] = [];
  disposeCalls = 0;
  text = '';
  path = '';
  version = 1;
  dispatch(bytes: Uint8Array): Uint8Array {
    const request = ToolingRequest.decode(bytes).request;
    let response: Response['response'];
    switch (request?.$case) {
      case 'open':
        this.sessions.add('p1');
        this.openedFiles = request.open.files.map((file) => file.path);
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
        this.moduleSpecifiers.push(request.module.specifier);
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
            runtimeId: '\0baml:p1:runtime',
            stale: false,
            watchFiles: [this.path],
          },
        };
        break;
      case 'close':
        this.closeCalls.push(request.close.projectId);
        response = {
          $case: 'closed',
          closed: { released: this.sessions.delete(request.close.projectId) },
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
  dispose(): void {
    this.disposeCalls++;
  }
  fingerprint() {
    return createHash('sha256').update(this.text).digest('hex');
  }
}

/** Resolves once the artifact cache holds at least one entry under
 * `directory`, whose layout is `<shard>/<key>.json`. */
async function waitForCachedArtifact(directory: string): Promise<void> {
  for (let attempt = 0; attempt < 400; attempt++) {
    const shards = await readdir(directory, { withFileTypes: true }).catch(
      () => [],
    );
    for (const shard of shards) {
      if (!shard.isDirectory()) continue;
      const names = await readdir(join(directory, shard.name)).catch(() => []);
      if (names.some((name) => name.endsWith('.json'))) return;
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 5));
  }
  throw new Error(`no BAML artifact was written under ${directory}`);
}

describe('project artifact cache', () => {
  it('hydrates hash-matching modules and misses after source changes', async () => {
    const root = await mkdtemp(join(tmpdir(), 'baml-project-cache-'));
    const cacheDir = join(root, 'cache');
    const source = join(root, 'main.baml');
    await writeFile(join(root, 'baml.toml'), '');
    await writeFile(source, 'class Person {}\n');

    const firstBackend = new TestBackend();
    const first = await BamlProject.open({
      backend: firstBackend,
      cacheDir,
      cwd: root,
    });
    expect(firstBackend.openedFiles).toContain(
      await realpath(join(root, 'baml.toml')),
    );
    first.resolveDts(source, source);
    expect(firstBackend.moduleCalls).toBe(1);
    // Artifacts are persisted fire-and-forget, so wait for the write to land
    // rather than racing a fixed sleep: a slow write would read as a cache
    // miss and fail the hydrate assertion below for reasons unrelated to it.
    await waitForCachedArtifact(join(cacheDir, projectId(root)));

    const hitBackend = new TestBackend();
    const hit = await BamlProject.open({
      backend: hitBackend,
      cacheDir,
      cwd: root,
    });
    hit.resolveDts(source, source);
    expect(hitBackend.moduleCalls).toBe(0);

    await writeFile(source, 'class Human {}\n');
    const missBackend = new TestBackend();
    const miss = await BamlProject.open({
      backend: missBackend,
      cacheDir,
      cwd: root,
    });
    miss.resolveDts(source, source);
    expect(missBackend.moduleCalls).toBe(1);
  });

  it('resolves bare specifiers to physical paths before asking the bridge', async () => {
    const root = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-project-resolve-')),
    );
    const dependency = join(root, 'node_modules', 'baml-dep');
    await mkdir(join(dependency, 'baml_src'), { recursive: true });
    await writeFile(
      join(dependency, 'package.json'),
      JSON.stringify({ name: 'baml-dep', version: '1.0.0' }),
    );
    await writeFile(
      join(dependency, 'baml_src', 'widget.baml'),
      'class Widget {}\n',
    );
    const source = join(root, 'main.baml');
    await writeFile(join(root, 'baml.toml'), '');
    await writeFile(source, 'class Person {}\n');

    const backend = new TestBackend();
    const project = await BamlProject.open({ backend, cwd: root });
    project.resolveDts('baml-dep/baml_src/widget.baml', join(root, 'index.ts'));
    // The bridge cannot run Node resolution; the specifier it receives must
    // already be the dependency's physical absolute path.
    expect(backend.moduleSpecifiers).toEqual([
      join(dependency, 'baml_src', 'widget.baml'),
    ]);
    // An unresolvable bare specifier fails loudly instead of querying the
    // bridge with a path joined onto the importer's directory.
    expect(() =>
      project.resolveDts(
        'missing-dep/baml_src/widget.baml',
        join(root, 'index.ts'),
      ),
    ).toThrow(/Could not resolve BAML import/);
  });

  it('releases the bridge session on dispose', async () => {
    const root = await mkdtemp(join(tmpdir(), 'baml-project-dispose-'));
    await writeFile(join(root, 'baml.toml'), '');
    await writeFile(join(root, 'main.baml'), 'class Person {}\n');

    const backend = new TestBackend();
    const project = await BamlProject.open({ backend, cwd: root });
    expect(backend.sessions.has('p1')).toBe(true);

    project.dispose();
    // Disposal is a protocol close, not a host-side flag: the bridge itself
    // dropped the session, so nothing holds the compiler database any more.
    expect(backend.closeCalls).toEqual(['p1']);
    expect(backend.sessions.has('p1')).toBe(false);
    expect(project.disposed).toBe(true);
    expect(backend.disposeCalls).toBe(1);

    // Idempotent: a host that disposes a lane it already replaced must not
    // send a second close, and must not throw on a teardown path.
    project.dispose();
    expect(backend.closeCalls).toEqual(['p1']);
    expect(backend.disposeCalls).toBe(1);
  });

  it('does not fail disposal when the bridge already dropped the session', async () => {
    const root = await mkdtemp(join(tmpdir(), 'baml-project-dispose-dead-'));
    await writeFile(join(root, 'baml.toml'), '');
    await writeFile(join(root, 'main.baml'), 'class Person {}\n');

    const backend = new TestBackend();
    const project = await BamlProject.open({ backend, cwd: root });
    backend.dispatch = () => {
      throw new Error('bridge is gone');
    };
    expect(() => project.dispose()).not.toThrow();
    expect(project.disposed).toBe(true);
  });
});
