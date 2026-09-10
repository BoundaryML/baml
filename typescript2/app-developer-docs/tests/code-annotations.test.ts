import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import test from 'node:test';
import { generateAnnotationAssets } from '../lib/snippets/annotation-assets';
import manifest from '../lib/snippets/annotation-manifest.json';
import { getAnnotationMetadata } from '../lib/snippets/annotation-metadata';
import { loadAnnotationSources } from '../lib/snippets/annotation-sources';

test('annotated images match current source, labels, and syntax highlighting', async () => {
  const sources = await loadAnnotationSources();
  assert.deepEqual(
    sources.map((source) => source.id).sort(),
    Object.keys(manifest).sort(),
  );
  for (const source of sources) {
    const { images, metadata } = await generateAnnotationAssets(source);
    assert.deepEqual(getAnnotationMetadata(source.id, source.code), metadata);
    for (const image of images) {
      const path = resolve(
        'public/book/annotations',
        `${source.id}-${image.theme}.svg`,
      );
      assert.equal(
        await readFile(path, 'utf8'),
        image.svg,
        `${source.id}: run pnpm docs:annotations:generate`,
      );
    }
  }
});

test('the page rejects an image when its displayed source changes', async () => {
  const [source] = await loadAnnotationSources();
  assert.throws(
    () => getAnnotationMetadata(source.id, `${source.code}\n`),
    /out of date/,
  );
});
