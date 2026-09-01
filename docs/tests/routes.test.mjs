import assert from 'node:assert/strict';
import { access, readFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const topLevelRoutes = ['baml', 'cli', 'bws', 'tutorials', 'examples', 'integrations'];

test('the public route contract has a landing page for every section', async () => {
  await Promise.all(
    topLevelRoutes.map((route) =>
      access(path.join(packageRoot, 'content', route, 'index.mdx')),
    ),
  );
});

test('the BAML book remains under /baml/book', async () => {
  await access(path.join(packageRoot, 'content', 'baml', 'book', 'index.mdx'));
});

test('content is served from the domain root', async () => {
  const source = await readFile(path.join(packageRoot, 'lib', 'source.ts'), 'utf8');
  assert.match(source, /baseUrl:\s*['"]\/['"]/);
});

test('highlighting consumes the monorepo BAML grammar', async () => {
  const config = await readFile(path.join(packageRoot, 'source.config.ts'), 'utf8');
  assert.match(config, /typescript2\/pkg-grammar\/baml\.tmLanguage\.json/);
});
