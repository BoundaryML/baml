import { readFile } from 'node:fs/promises';
import { join } from 'node:path';

const requiredSelectors = [
  '.language-tabs-list',
  '.language-tabs-logo',
  '.annotated-code-actions',
  '.annotated-code-image',
  '.annotated-light',
  '.annotated-dark',
  '.new-concepts',
  '.mobile-nav-dialog',
];

/** Check the CSS linked by the built page, not source files or unused chunks. */
export async function validateBuiltStyles(buildDirectory: string) {
  const html = await readFile(
    join(buildDirectory, 'server/app/baml/book/errors.html'),
    'utf8',
  );
  const stylesheets = new Set(
    [...html.matchAll(/<link\b[^>]*>/g)]
      .filter(([tag]) => /\brel="stylesheet"/.test(tag))
      .map(([tag]) => tag.match(/\bhref="([^"]+)"/)?.[1])
      .filter((href): href is string => Boolean(href)),
  );
  if (stylesheets.size === 0) {
    throw new Error('Built book page does not link any stylesheets.');
  }
  const css = (
    await Promise.all(
      [...stylesheets].map((href) => {
        const pathname = new URL(href, 'https://docs.invalid').pathname;
        if (!pathname.startsWith('/_next/static/')) {
          throw new Error(`Unexpected built stylesheet: ${href}`);
        }
        return readFile(
          join(buildDirectory, pathname.slice('/_next/'.length)),
          'utf8',
        );
      }),
    )
  ).join('\n');
  const missing = requiredSelectors.filter(
    (selector) => !css.includes(selector),
  );
  if (missing.length > 0) {
    throw new Error(
      `Built book stylesheet is missing ${missing.join(', ')}. Refusing to deploy stale CSS.`,
    );
  }
}
