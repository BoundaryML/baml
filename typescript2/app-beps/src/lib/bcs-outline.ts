// A BCS proposal's pages as a tree: the main page first, then sub-pages under their parents in the
// order they were saved.

export type OutlinePage = { slug: string; title: string; parentSlug?: string; format: string };
export type OutlineEntry = OutlinePage & { depth: number };

/** The pages in reading order, each with its depth: main page at 0, top-level sub-pages at 1. */
export function flattenOutline(pages: OutlinePage[]): OutlineEntry[] {
  const children = new Map<string, OutlinePage[]>();
  for (const page of pages) {
    if (page.slug === "") continue;
    const parent = page.parentSlug ?? "";
    children.set(parent, [...(children.get(parent) ?? []), page]);
  }
  const main = pages.find((page) => page.slug === "");
  const result: OutlineEntry[] = main ? [{ ...main, depth: 0 }] : [];
  const visit = (slug: string, depth: number) => {
    for (const child of children.get(slug) ?? []) {
      result.push({ ...child, depth });
      visit(child.slug, depth + 1);
    }
  };
  visit("", 1);
  return result;
}

/**
 * The sub-pages `slug` could move under: every sub-page except itself and those inside it, with
 * depth counted from the top level.
 */
export function parentChoices(pages: OutlinePage[], slug?: string) {
  const inside = new Set<string>(slug ? [slug] : []);
  let grew = true;
  while (grew) {
    grew = false;
    for (const page of pages) {
      if (page.parentSlug && inside.has(page.parentSlug) && !inside.has(page.slug)) {
        inside.add(page.slug);
        grew = true;
      }
    }
  }
  return flattenOutline(pages)
    .filter((page) => page.slug !== "" && !inside.has(page.slug))
    .map((page) => ({ slug: page.slug, title: page.title, depth: page.depth - 1 }));
}
