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
  await Promise.all([
    access(path.join(packageRoot, 'content', 'baml', 'book', 'index.mdx')),
    access(path.join(packageRoot, 'content', 'baml', 'book', 'meta.json')),
    access(path.join(packageRoot, 'book-import.json')),
    access(path.join(packageRoot, 'generated', 'book', 'manifest.json')),
  ]);
});

test('content is served from the domain root', async () => {
  const source = await readFile(path.join(packageRoot, 'lib', 'source.ts'), 'utf8');
  assert.match(source, /baseUrl:\s*['"]\/['"]/);
});

test('highlighting consumes the monorepo BAML grammar', async () => {
  const config = await readFile(path.join(packageRoot, 'source.config.ts'), 'utf8');
  assert.match(config, /typescript2\/pkg-grammar\/baml\.tmLanguage\.json/);
});

test('generated standard-library packages are part of the language route tree', async () => {
  await Promise.all([
    access(path.join(packageRoot, 'content', 'baml', 'language', 'reference', 'index.md')),
    access(path.join(packageRoot, 'content', 'baml', 'language', 'reference', 'baml', 'classes', 'Array.md')),
    access(path.join(packageRoot, 'content', 'baml', 'language', 'reference', 'baml', 'classes', 'http', 'Request.md')),
    access(path.join(packageRoot, 'content', 'baml', 'language', 'reference', 'ai', 'index.md')),
    access(path.join(packageRoot, 'content', 'baml', 'language', 'reference', 'reflect', 'index.md')),
    access(path.join(packageRoot, 'content', 'baml', 'language', 'reference', 'testing', 'index.md')),
    access(path.join(packageRoot, 'content', 'baml', 'language', 'reference', 'assert', 'index.md')),
    access(path.join(packageRoot, 'generated', 'baml', 'manifest.json')),
  ]);
});

test('generated CLI reference includes nested public commands', async () => {
  await Promise.all([
    access(path.join(packageRoot, 'content', 'cli', 'commands', 'index.md')),
    access(path.join(packageRoot, 'content', 'cli', 'commands', 'auth', 'index.md')),
    access(path.join(packageRoot, 'content', 'cli', 'commands', 'auth', 'login.md')),
    access(path.join(packageRoot, 'generated', 'cli', 'manifest.json')),
  ]);
});

test('runnable examples ship the stable worker entrypoint but no derived runtime', async () => {
  await Promise.all([
    access(path.join(packageRoot, 'content', 'examples', 'runnable-baml.mdx')),
    access(path.join(packageRoot, 'public', 'baml-runtime', 'runner-worker.mjs')),
  ]);
  const trackedRuntime = await import('node:child_process').then(({ execFileSync }) =>
    execFileSync('git', ['ls-files', '--', 'docs/public/baml-runtime/manifest.json', 'docs/public/baml-runtime/artifacts'], {
      cwd: path.resolve(packageRoot, '..'),
      encoding: 'utf8',
    }).trim(),
  );
  assert.equal(trackedRuntime, '');
});
