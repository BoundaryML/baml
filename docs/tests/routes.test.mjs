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

test('generated package reference is part of the language route tree', async () => {
  await Promise.all([
    access(path.join(packageRoot, 'content', 'baml', 'language', 'reference', 'index.md')),
    access(path.join(packageRoot, 'content', 'baml', 'language', 'reference', 'classes', 'Array.md')),
    access(path.join(packageRoot, 'content', 'baml', 'language', 'reference', 'functions', 'env', 'ref.md')),
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

test('runnable examples ship a worker and a content-addressed runtime', async () => {
  const manifest = JSON.parse(
    await readFile(path.join(packageRoot, 'public', 'baml-runtime', 'manifest.json'), 'utf8'),
  );
  assert.match(manifest.wasm, /^\/baml-runtime\/artifacts\/[0-9a-f]+\/bridge_wasm_bg\.wasm$/);
  await Promise.all([
    access(path.join(packageRoot, 'content', 'examples', 'runnable-baml.mdx')),
    access(path.join(packageRoot, 'public', 'baml-runtime', 'runner-worker.mjs')),
    access(path.join(packageRoot, 'public', manifest.wasm.slice(1))),
  ]);
});
