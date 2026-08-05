import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { isBamlSidecar } from '@boundaryml/baml-tooling';
import { describe, expect, it } from 'vitest';

const workspace = resolve(import.meta.dirname, '../..');

describe('packed technology surface', () => {
  it('ships all hosts from the one build core', () => {
    const manifest = JSON.parse(
      readFileSync(resolve(workspace, 'pkg-baml-tooling/package.json'), 'utf8'),
    );
    for (const host of ['vite', 'rollup', 'webpack', 'esbuild', 'bun'])
      expect(manifest.exports[`./${host}`]).toBeDefined();
    expect(
      existsSync(resolve(workspace, 'pkg-baml-tooling/src/build-core.ts')),
    ).toBe(true);
  });

  it('ships a real CJS tsserver entry and sidecar generator', () => {
    const tooling = JSON.parse(
      readFileSync(resolve(workspace, 'pkg-baml-tooling/package.json'), 'utf8'),
    );
    expect(tooling.exports['./typescript-plugin'].require).toMatch(/\.cjs$/);
    expect(tooling.peerDependencies.typescript).toBe('>=5.5');
    expect(tooling.bin['baml-ts-gen']).toBeDefined();
    expect(isBamlSidecar('resume.baml.d.ts')).toBe(true);
  });

  it('keeps all cross-language locations on physical BAML paths', () => {
    const source = readFileSync(
      resolve(workspace, 'pkg-baml-tooling/src/typescript-plugin.cts'),
      'utf8',
    );
    expect(source).toContain('this.virtualFiles.has');
    expect(source).toContain('mapGenerated');
    expect(source).toContain('definitionAt');
    expect(source).not.toContain('return output.push(definition) // virtual');
  });
});
