import assert from 'node:assert/strict';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { validateBuiltStyles } from '../lib/built-styles';

test('the stylesheet gate inspects linked CSS from SSR HTML, not unused build chunks', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'book-styles-'));
  const completeCss = [
    '.language-tabs-list',
    '.language-tabs-logo',
    '.annotated-code-actions',
    '.annotated-code-image',
    '.annotated-light',
    '.annotated-dark',
    '.new-concepts',
    '.mobile-nav-dialog',
    '[data-book-perspective]',
    '.book-reading-options',
    '.book-reading-option',
  ]
    .map((selector) => `${selector}{display:block}`)
    .join('\n');
  try {
    await mkdir(join(directory, 'static/chunks'), { recursive: true });
    await writeFile(join(directory, 'static/chunks/unused.css'), completeCss);
    await writeFile(
      join(directory, 'static/chunks/linked.css'),
      '.language-tabs-list{}',
    );
    const html =
      '<html><head><link rel="stylesheet" href="/_next/static/chunks/linked.css"/></head><body>Book</body></html>';
    await assert.rejects(validateBuiltStyles(directory, html), /missing/);
    await writeFile(join(directory, 'static/chunks/linked.css'), completeCss);
    await assert.doesNotReject(validateBuiltStyles(directory, html));
    await assert.rejects(
      validateBuiltStyles(directory, '<html/>'),
      /does not link any stylesheets/,
    );
    await assert.rejects(
      validateBuiltStyles(
        directory,
        '<link rel="stylesheet" href="/other.css"/>',
      ),
      /Unexpected built stylesheet/,
    );
  } finally {
    await rm(directory, { force: true, recursive: true });
  }
});
