import { parseBookPerspective } from './book-perspectives';

/** Paths are relative to content/baml/book. No frontmatter controls routing. */
export function bookFileIdentity(path: string) {
  const shared = /^([a-z0-9]+(?:-[a-z0-9]+)*)\.mdx$/.exec(path);
  if (shared) {
    return {
      chapter: shared[1] === 'index' ? '/baml/book' : `/baml/book/${shared[1]}`,
      kind: 'shared' as const,
      path,
      perspective: 'default' as const,
    };
  }
  const version = /^([a-z0-9]+(?:-[a-z0-9]+)*)\/([a-z0-9-]+)\.mdx$/.exec(path);
  if (!version || version[1] === 'index') {
    throw new Error(
      `Invalid book path: ${path}. Use <chapter>.mdx or <chapter>/<perspective>.mdx.`,
    );
  }
  const perspective = parseBookPerspective(version[2]);
  if (!perspective) throw new Error(`Unknown book perspective in ${path}`);
  return {
    chapter: `/baml/book/${version[1]}`,
    kind: 'perspective' as const,
    path,
    perspective,
  };
}

export function validateBookLayout(paths: readonly string[]) {
  const files = paths.map(bookFileIdentity);
  const shared = new Set(
    files.filter((file) => file.kind === 'shared').map((file) => file.chapter),
  );
  const defaults = new Set(
    files
      .filter(
        (file) => file.kind === 'perspective' && file.perspective === 'default',
      )
      .map((file) => file.chapter),
  );
  const seen = new Set<string>();
  for (const file of files) {
    if (seen.has(file.path))
      throw new Error(`Duplicate book file: ${file.path}`);
    seen.add(file.path);
    if (file.kind === 'shared') continue;
    if (shared.has(file.chapter))
      throw new Error(
        `Book chapter has both a file and a perspective directory: ${file.chapter}`,
      );
    if (!defaults.has(file.chapter))
      throw new Error(
        `Book perspective directory requires default.mdx: ${file.chapter}`,
      );
  }
  return files;
}
