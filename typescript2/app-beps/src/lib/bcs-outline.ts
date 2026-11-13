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

export type Heading = { id: string; text: string; depth: number };
export type Section = Heading & { children: Heading[] };

/**
 * The page's headings as sections: the page's single h1 (its title, already in the header) is left
 * out, the shallowest remaining level makes the sections, and the next level their sub-sections.
 */
export function sections(headings: Heading[]): Section[] {
  const titles = headings.filter((each) => each.depth === 1);
  const rest = titles.length === 1 ? headings.filter((each) => each !== titles[0]) : headings;
  if (rest.length === 0) return [];
  const top = Math.min(...rest.map((each) => each.depth));
  const result: Section[] = [];
  for (const heading of rest) {
    if (heading.depth === top || result.length === 0) result.push({ ...heading, children: [] });
    else if (heading.depth === top + 1) result[result.length - 1].children.push(heading);
  }
  return result;
}
