import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { relative, resolve, sep } from 'node:path';
import test from 'node:test';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { ChangelogContent } from '../components/changelog-content.tsx';
import {
  changelogVersionId,
  loadCanonicalChangelog,
  parseCanonicalChangelog,
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

test('the changelog renders directly from the complete canonical source', async () => {
  const canonicalPath = resolve(process.cwd(), '..', '..', 'CHANGELOG.md');
  const changelog = await loadCanonicalChangelog();
  const canonicalSource = await readFile(canonicalPath, 'utf8');
  const firstVersion = canonicalSource.match(/^## \[([^\]]+)\]/m)?.[1];

  assert.equal(changelog.sourcePath, canonicalPath);
  assert.equal(changelog.entries[0]?.version, firstVersion);
  assert.ok(changelog.entries.length > 100);
  assert.equal(
    changelog.headingIds.filter((id) => id !== undefined).length,
    changelog.entries.length,
  );
  assert.equal(
    new Set(changelog.entries.map(({ id }) => id)).size,
    changelog.entries.length,
  );
  assert.doesNotMatch(changelog.markdown, /^# Changelog$/m);

  const rendered = renderToStaticMarkup(
    createElement(ChangelogContent, {
      headingIds: changelog.headingIds,
      markdown: changelog.markdown,
    }),
  );
  assert.match(rendered, new RegExp(`id="${changelog.entries[0]?.id}"`));
});

test('legacy non-version level-two headings do not shift release anchors', () => {
  const changelog = parseCanonicalChangelog(
    [
      '# Changelog',
      '',
      '## [1.2.0](https://example.com/1.2.0) - 2026-01-02',
      '',
      '## Migration notes',
      '',
      '## [1.1.0] - 2026-01-01',
    ].join('\n'),
    '/canonical/CHANGELOG.md',
  );

  assert.deepEqual(changelog.headingIds, [
    changelogVersionId('1.2.0'),
    undefined,
    changelogVersionId('1.1.0'),
  ]);
  const rendered = renderToStaticMarkup(
    createElement(ChangelogContent, {
      headingIds: changelog.headingIds,
      markdown: changelog.markdown,
    }),
  );
  assert.match(rendered, /<h2 id="v1-2-0">/);
  assert.match(rendered, /<h2>Migration notes<\/h2>/);
  assert.match(rendered, /<h2 id="v1-1-0">/);
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
