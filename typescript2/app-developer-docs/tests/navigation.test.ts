import assert from 'node:assert/strict';
import { access, readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import test from 'node:test';

import {
  type DocumentationLink,
  documentationNavigation,
  documentationPages,
  primaryNavigation,
  searchablePages,
} from '../lib/navigation.ts';

test('every shell navigation destination has a concrete static route', async () => {
  const hrefs = new Set([
    '/',
    ...primaryNavigation.map((item) => item.href),
    ...documentationPages.map((item) => item.href),
    ...searchablePages.map((item) => item.href),
  ]);

  for (const href of hrefs) {
    assert.match(href, /^\/[a-z0-9/-]*$/);
    const pagePath = href === '/' ? 'app/page.tsx' : `app${href}/page.tsx`;
    await access(resolve(process.cwd(), pagePath));
  }
});

test('navigation says Cloud while product content keeps the official name', async () => {
  const primaryCloud = primaryNavigation.find((item) => item.href === '/bcs');
  const sidebarCloud = searchablePages.find((item) => item.href === '/bcs');
  const cloudPage = await readFile(
    resolve(process.cwd(), 'content/bcs/index.mdx'),
    'utf8',
  );

  assert.equal(primaryCloud?.label, 'Cloud');
  assert.equal(sidebarCloud?.label, 'Cloud');
  assert.match(cloudPage, /title: Boundary Cloud Services/);
});

test('nested navigation preserves canonical path ancestry and unique routes', () => {
  const hrefs = new Set<string>();

  const visit = (links: DocumentationLink[], parentHref?: string) => {
    for (const link of links) {
      assert.ok(
        !hrefs.has(link.href),
        `Duplicate navigation route: ${link.href}`,
      );
      hrefs.add(link.href);
      if (parentHref) assert.ok(link.href.startsWith(`${parentHref}/`));
      visit(link.children ?? [], link.href);
    }
  };

  for (const group of documentationNavigation) visit(group.links);

  assert.deepEqual(
    [...hrefs],
    documentationPages.slice(1).map((page) => page.href),
  );
  assert.deepEqual(
    [...hrefs],
    searchablePages.map((page) => page.href),
  );
});

test('desktop navigation branches do not replay open animations on route changes', async () => {
  const sidebarSource = await readFile(
    resolve(process.cwd(), 'components/docs-sidebar.tsx'),
    'utf8',
  );

  assert.doesNotMatch(
    sidebarSource,
    /transition-\[grid-template-rows,opacity\]/,
  );
  assert.doesNotMatch(sidebarSource, /transition-transform duration-200/);
});

test('unknown routes use the branded portal 404', async () => {
  const notFoundSource = await readFile(
    resolve(process.cwd(), 'app/not-found.tsx'),
    'utf8',
  );

  assert.match(notFoundSource, /Boundary developer documentation/);
  assert.match(notFoundSource, /This documentation page does not exist/);
  assert.match(notFoundSource, /href="\/baml"/);
  assert.doesNotMatch(notFoundSource, /vercel/i);
});

test('the docs shell preserves the measured shadcn geometry', async () => {
  const [shellSource, headerSource, globalStyles] = await Promise.all([
    readFile(resolve(process.cwd(), 'components/docs-shell.tsx'), 'utf8'),
    readFile(resolve(process.cwd(), 'components/site-header.tsx'), 'utf8'),
    readFile(resolve(process.cwd(), 'app/globals.css'), 'utf8'),
  ]);

  assert.match(shellSource, /\[--sidebar-width:18rem\]/);
  assert.match(shellSource, /max-w-160/);
  assert.match(shellSource, /sm:text-\[15px\]/);
  assert.match(shellSource, /w-\[var\(--sidebar-width\)\]/);
  assert.match(headerSource, /container-wrapper px-6/);
  assert.doesNotMatch(headerSource, /border-b/);
  assert.doesNotMatch(globalStyles, /fumadocs-ui\/css/);
});
