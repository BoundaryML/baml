const versionedReferenceRoots = [
  ['baml', 'language', 'reference'],
  ['cli', 'commands'],
];

function matchesRoot(slug, root) {
  return root.every((segment, index) => slug[index] === segment);
}

/**
 * Version landing pages are cheap and useful to pre-render, but eagerly rendering
 * every symbol and command once per toolchain makes release builds grow linearly.
 * Omitted paths remain available through Next.js dynamic params and are cached
 * after their first request.
 *
 * @param {string[]} versions
 */
export function createDocsPrerenderPredicate(versions) {
  const versionSegments = new Set(versions.map((version) => `v${version}`));

  /** @param {string[]} slug */
  return function shouldPreRenderDocsSlug(slug) {
    const root = versionedReferenceRoots.find((candidate) => matchesRoot(slug, candidate));
    if (!root) return true;

    const versionSegment = slug[root.length];
    if (!versionSegments.has(versionSegment)) return true;

    return slug.length === root.length + 1;
  };
}
