import assert from 'node:assert/strict';
import { access, readdir, readFile } from 'node:fs/promises';
import { relative, resolve, sep } from 'node:path';
import test from 'node:test';
import {
  changelogVersionId,
  loadCanonicalChangelog,
} from '../lib/changelog/loader.ts';
import { bridgeDataSchema, loadBridgeData } from '../lib/content/bridges.ts';

const expectedAuthoredRoutes = [
  '/baml',
  '/baml/book',
  '/baml/book/foundations',
  '/baml/book/foundations/functions',
  '/baml/bridges',
  '/baml/bridges/typescript',
  '/baml/get-started',
  '/baml/language',
  '/baml/language/functions',
  '/bcs',
  '/cli',
  '/examples',
  '/examples/classify-support-tickets',
  '/tutorials',
  '/tutorials/structured-extraction',
];

async function collectFiles(directory: string): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const path = resolve(directory, entry.name);
      return entry.isDirectory() ? collectFiles(path) : [path];
    }),
  );
  return nested.flat();
}

test('the MDX collection contains exactly the authored route contract', async () => {
  const contentRoot = resolve(process.cwd(), 'content');
  const routes = (await collectFiles(contentRoot))
    .filter((path) => path.endsWith('.mdx'))
    .map((path) => {
      const segments = relative(contentRoot, path)
        .split(sep)
        .map((segment) => segment.replace(/\.mdx$/, ''));
      if (segments.at(-1) === 'index') segments.pop();
      return `/${segments.join('/')}`;
    })
    .sort();
  assert.deepEqual(routes, expectedAuthoredRoutes);
});

test('structured bridge data is strict, complete, and path confined', async () => {
  const bridge = await loadBridgeData('typescript');
  assert.equal(bridge.schemaVersion, 1);
  assert.ok(bridge.compatibility.length > 0);
  assert.ok(bridge.types.length > 0);
  assert.ok(bridge.transitions.length > 0);
  assert.ok(bridge.gotchas.length > 0);
  assert.equal(
    bridgeDataSchema.safeParse({ ...bridge, undocumented: true }).success,
    false,
  );
  await assert.rejects(loadBridgeData('../typescript'), /Invalid bridge ID/);
});

test('the changelog is loaded directly from the canonical language source', async () => {
  const changelog = await loadCanonicalChangelog();
  assert.match(changelog.sourcePath, /baml_language\/CHANGELOG\.md$/);
  assert.equal(changelog.entries[0]?.version, '0.18.0');
  assert.equal(changelog.entries[0]?.id, 'v0-18-0');
  assert.equal(changelogVersionId('1.2.3'), 'v1-2-3');
  assert.match(changelog.markdown, /^## \[0\.18\.0\]/);
  assert.doesNotMatch(changelog.markdown, /^# BAML Language changelog/m);
  await assert.rejects(
    access(resolve(process.cwd(), 'content', 'changelog.md')),
  );
});

test('authored MDX never embeds a second BAML source block', async () => {
  const files = (await collectFiles(resolve(process.cwd(), 'content'))).filter(
    (path) => path.endsWith('.mdx'),
  );
  for (const path of files) {
    const source = await readFile(path, 'utf8');
    assert.doesNotMatch(source, /```baml/i, path);
  }
});
