import assert from 'node:assert/strict';
import { readdir, readFile } from 'node:fs/promises';
import { relative, resolve, sep } from 'node:path';
import test from 'node:test';
import { z } from 'zod';
import { validateBookLayout } from '../lib/content/book-layout';
import { bridgeDataSchema, loadBridgeData } from '../lib/content/bridges.ts';
import {
  loadProjectSnippet,
  loadStandaloneSnippet,
} from '../lib/snippets/discovery';
import { selectProjectFiles } from '../lib/snippets/selection';

const expectedAuthoredRoutes = [
  '/baml',
  '/baml/book',
  '/baml/book/common-programming-concepts',
  '/baml/book/concurrency',
  '/baml/book/errors',
  '/baml/book/interfaces',
  '/baml/bridges',
  '/baml/bridges/typescript',
  '/baml/get-started',
  '/baml/language',
  '/baml/language/functions',
  '/bcs',
  '/cli',
  '/examples',
  '/examples/classify-support-tickets',
  '/examples/vision',
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
  const files = (await collectFiles(contentRoot))
    .filter((path) => path.endsWith('.mdx'))
    .map((path) => relative(contentRoot, path).split(sep).join('/'));
  const bookPrefix = 'baml/book/';
  const book = validateBookLayout(
    files
      .filter((path) => path.startsWith(bookPrefix))
      .map((path) => path.slice(bookPrefix.length)),
  );
  const routes = [
    ...new Set(book.map((file) => file.chapter)),
    ...files
      .filter((path) => !path.startsWith(bookPrefix))
      .map((path) => {
        const segments = path.replace(/\.mdx$/, '').split('/');
        if (segments.at(-1) === 'index') segments.pop();
        return `/${segments.join('/')}`;
      }),
  ].sort();
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

test('authored MDX never embeds a second BAML source block', async () => {
  const files = (await collectFiles(resolve(process.cwd(), 'content'))).filter(
    (path) => path.endsWith('.mdx'),
  );
  for (const path of files) {
    const source = await readFile(path, 'utf8');
    assert.doesNotMatch(source, /```baml/i, path);
  }
});

test('authored excerpts resolve to canonical project regions and internal links resolve', async () => {
  const files = (await collectFiles(resolve(process.cwd(), 'content'))).filter(
    (path) => path.endsWith('.mdx'),
  );
  for (const path of files) {
    const source = await readFile(path, 'utf8');
    for (const match of source.matchAll(
      /<BamlSnippet id="([^"]+)"(?: region="([^"]+)")?\s*\/>/g,
    )) {
      const snippet = await loadStandaloneSnippet(match[1]);
      if (match[2])
        assert.ok(
          snippet.parsed.regions.has(match[2]),
          `${path}: missing region ${match[2]}`,
        );
    }
    for (const match of source.matchAll(
      /<BamlProject id="([^"]+)"(?: file="([^"]+)" regions=\{(\[[^\]]+\])\})?(?: annotation="[^"]+")?\s*\/>/g,
    )) {
      const project = await loadProjectSnippet(match[1]);
      const regions = match[3]
        ? z.array(z.string()).parse(JSON.parse(match[3]))
        : undefined;
      const displayed = selectProjectFiles(project, match[2], regions);
      assert.ok(displayed.length > 0, path);
      for (const file of displayed) {
        assert.ok(file.displaySource.trim(), path);
        assert.doesNotMatch(
          file.displaySource,
          /docs:start|docs:end|ANCHOR/,
          path,
        );
      }
    }
    for (const match of source.matchAll(
      /\]\((\/[^)#]+)(?:#[^)]*)?\)|href="(\/[^"#]+)"/g,
    )) {
      const href = match[1] ?? match[2];
      if (href.startsWith('/examples/vision/')) {
        const target =
          href === '/examples/vision/source'
            ? resolve(process.cwd(), 'app/examples/vision/source/route.ts')
            : resolve(process.cwd(), `public${href}`);
        await readFile(target);
        continue;
      }
      assert.ok(
        ['/', '/baml/packages', ...expectedAuthoredRoutes].includes(href),
        `${path}: broken authored link ${href}`,
      );
    }
  }
});
