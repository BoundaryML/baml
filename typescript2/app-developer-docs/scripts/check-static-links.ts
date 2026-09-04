import { readdir, readFile, stat } from 'node:fs/promises';
import { extname, relative, resolve, sep } from 'node:path';

const outputRoot = resolve(import.meta.dirname, '..', 'out');
const hrefPattern = /\shref=(?:"([^"]+)"|'([^']+)')/g;
const idPattern = /\sid=(?:"([^"]+)"|'([^']+)')/g;
const sitemapLocationPattern = /<loc>([^<]+)<\/loc>/g;

async function collectHtmlFiles(directory: string): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const path = resolve(directory, entry.name);
      if (entry.isDirectory()) return collectHtmlFiles(path);
      return extname(entry.name) === '.html' ? [path] : [];
    }),
  );
  return files.flat();
}

function fileRoute(path: string): string {
  const relativePath = relative(outputRoot, path).split(sep).join('/');
  if (relativePath === 'index.html') return '/';
  return `/${relativePath.replace(/\.html$/, '')}`;
}

function internalTarget(href: string, currentRoute: string) {
  if (href.startsWith('#')) {
    return { fragment: href.slice(1), route: currentRoute };
  }
  if (!href.startsWith('/') || href.startsWith('//')) return null;
  const parsed = new URL(href, 'https://developer.boundaryml.com');
  if (parsed.pathname.startsWith('/_next/')) return null;
  const route =
    parsed.pathname === '/' ? '/' : parsed.pathname.replace(/\/$/, '');
  return { fragment: parsed.hash.slice(1), route };
}

const files = await collectHtmlFiles(outputRoot);
const pages = new Map<
  string,
  { anchors: Set<string>; file: string; html: string }
>();
for (const file of files) {
  const route = fileRoute(file);
  const html = await readFile(file, 'utf8');
  pages.set(route, {
    anchors: new Set(
      [...html.matchAll(idPattern)].map((match) => match[1] ?? match[2]),
    ),
    file,
    html,
  });
}

async function resolveTarget(route: string) {
  const page = pages.get(route);
  if (page) return page;

  const htmlPath = resolve(outputRoot, `.${route}.html`);
  const html = await readFile(htmlPath, 'utf8').catch(() => null);
  if (html !== null) {
    return {
      anchors: new Set(
        [...html.matchAll(idPattern)].map((match) => match[1] ?? match[2]),
      ),
      file: htmlPath,
      html,
    };
  }

  const assetPath = resolve(outputRoot, `.${route}`);
  return stat(assetPath)
    .then((value) =>
      value.isFile()
        ? { anchors: new Set<string>(), file: assetPath, html: '' }
        : null,
    )
    .catch(() => null);
}

const failures = new Set<string>();
const routeLinks = new Map<string, Set<string>>();
for (const [currentRoute, page] of pages) {
  const links = new Set<string>();
  routeLinks.set(currentRoute, links);
  for (const match of page.html.matchAll(hrefPattern)) {
    const href = match[1] ?? match[2];
    const target = internalTarget(href, currentRoute);
    if (!target) continue;
    const targetPage = await resolveTarget(target.route);
    if (!targetPage) {
      failures.add(`${relative(outputRoot, page.file)}: missing route ${href}`);
      continue;
    }
    if (pages.has(target.route)) links.add(target.route);
    if (
      target.fragment &&
      !targetPage.anchors.has(decodeURIComponent(target.fragment))
    ) {
      failures.add(
        `${relative(outputRoot, page.file)}: missing anchor ${href}`,
      );
    }
  }
}

const reachableRoutes = new Set(['/']);
const pendingRoutes = ['/'];
while (pendingRoutes.length > 0) {
  const route = pendingRoutes.shift();
  if (!route) continue;
  for (const target of routeLinks.get(route) ?? []) {
    if (reachableRoutes.has(target)) continue;
    reachableRoutes.add(target);
    pendingRoutes.push(target);
  }
}
for (const route of pages.keys()) {
  if (route === '/404' || route === '/_not-found') continue;
  if (!reachableRoutes.has(route)) {
    failures.add(`${route}: static page is unreachable from /`);
  }
}

const sitemapSource = await readFile(
  resolve(outputRoot, 'sitemap.xml'),
  'utf8',
);
const sitemapRoutes = new Set(
  [...sitemapSource.matchAll(sitemapLocationPattern)].flatMap((match) => {
    const location = match[1];
    if (!location) return [];
    const route = new URL(location).pathname;
    return [route === '/' ? '/' : route.replace(/\/$/, '')];
  }),
);
const excludedFromSitemap = new Set(['/404', '/_not-found']);
for (const [route, page] of pages) {
  if (excludedFromSitemap.has(route)) continue;
  const noindex = /<meta[^>]+name="robots"[^>]+content="[^"]*noindex/i.test(
    page.html,
  );
  if (!noindex && !sitemapRoutes.has(route)) {
    failures.add(`${route}: indexable page is missing from sitemap.xml`);
  }
  if (noindex && sitemapRoutes.has(route)) {
    failures.add(`${route}: noindex page is present in sitemap.xml`);
  }
}
for (const route of sitemapRoutes) {
  if (!pages.has(route)) {
    failures.add(`sitemap.xml: missing static page ${route}`);
  }
}

if (failures.size > 0) {
  throw new Error(
    `Static internal-link validation failed:\n${[...failures].join('\n')}`,
  );
}

console.log(
  `Validated links, reachability, and sitemap coverage across ${pages.size} static HTML pages.`,
);
