import assert from 'node:assert/strict';
import { compile } from '@mdx-js/mdx';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { convertChapter } from '../scripts/import-baml-book.mjs';

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sourceRoot = path.join(packageRoot, 'tests', 'fixtures', 'book');
const source = 'src/chapter.md';

async function approvedChapter(overrides = {}) {
  const raw = await readFile(path.join(sourceRoot, source), 'utf8');
  return {
    source,
    output: 'getting-started.mdx',
    status: 'approved',
    sourceSha256: createHash('sha256').update(raw).digest('hex'),
    ...overrides,
  };
}

test('converts mdBook semantics into native Fumadocs MDX', async () => {
  const result = await convertChapter({ chapter: await approvedChapter(), sourceRoot });

  assert.match(result.content, /title: "Getting Started"/);
  assert.match(result.content, /<Callout title="Note">/);
  assert.match(result.content, /<BookListing number="1-1" fileName="baml_src\/main\.baml"/);
  assert.match(result.content, /caption=\{<>A function that returns <em>its argument<\/em><\/>\}/);
  assert.match(result.content, /<BamlRunner files=\{/);
  assert.match(result.content, /showSource=\{false\}/);
  assert.match(result.content, /<BookQuiz questions=\{/);
  assert.match(result.content, /"type":"Tracing"/);
  const renderedListing = result.content.match(/```baml\n([\s\S]*?)```/)?.[1] ?? '';
  assert.doesNotMatch(renderedListing, /ANCHOR/);
  assert.match(result.content, /"baml_src\/main\.baml":"\/\/ ANCHOR: all/);
  assert.doesNotMatch(result.content, /\{\{#(?:include|quiz)/);
  await assert.doesNotReject(
    compile(result.content.replace(/^---[\s\S]*?---\n/, '')),
    'the converted chapter must parse as MDX',
  );
});

test('refuses source that changed after editorial approval', async () => {
  await assert.rejects(
    convertChapter({
      chapter: await approvedChapter({ sourceSha256: '0'.repeat(64) }),
      sourceRoot,
    }),
    /Approval hash mismatch/,
  );
});
