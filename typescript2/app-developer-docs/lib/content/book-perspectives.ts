import { languageTabDefinitions } from './language-tabs';

export const BOOK_PERSPECTIVE_COOKIE = 'baml-book-perspective';
export const BOOK_PERSPECTIVE_PARAM = 'perspective';

export const bookPerspectiveDefinitions = {
  default: {
    label: 'Start with the basics',
    logo: languageTabDefinitions.BAML.logo,
    shortLabel: 'The basics',
  },
  typescript: {
    label: 'Coming from TypeScript',
    logo: languageTabDefinitions.TypeScript.logo,
    shortLabel: 'TypeScript',
  },
} as const;

export type BookPerspective = keyof typeof bookPerspectiveDefinitions;
export const bookPerspectiveIds = Object.keys(
  bookPerspectiveDefinitions,
).filter((key): key is BookPerspective =>
  Object.hasOwn(bookPerspectiveDefinitions, key),
);

export type SectionKeys = Record<string, string>;

export function parseBookPerspective(
  value: string | null | undefined,
): BookPerspective | undefined {
  return bookPerspectiveIds.find((perspective) => perspective === value);
}

export function resolveBookPerspective(
  requested: BookPerspective,
  available: readonly BookPerspective[],
): BookPerspective {
  return available.includes(requested) ? requested : 'default';
}

/** Keys are optional semantic matches, while values are each version's heading IDs. */
export function matchingSection(
  heading: string | undefined,
  previous: SectionKeys,
  next: SectionKeys,
  nextHeadings: readonly string[],
): string | undefined {
  if (!heading) return undefined;
  const key = Object.keys(previous).find((key) => previous[key] === heading);
  const mapped = key ? next[key] : undefined;
  if (mapped && nextHeadings.includes(mapped)) return mapped;
  return nextHeadings.includes(heading) ? heading : undefined;
}

export function perspectiveHref(href: string, perspective: BookPerspective) {
  const url = new URL(href, 'https://book.invalid');
  url.searchParams.set(BOOK_PERSPECTIVE_PARAM, perspective);
  return `${url.pathname}${url.search}${url.hash}`;
}

export interface BookVersionMetadata {
  chapter: string;
  perspective: BookPerspective;
  headings: readonly string[];
  sectionKeys?: SectionKeys;
}

/** Called while loading content and by CI, so broken mappings fail before publication. */
export function validateBookVersions(versions: readonly BookVersionMetadata[]) {
  const seen = new Set<string>();
  for (const version of versions) {
    const id = `${version.chapter}:${version.perspective}`;
    if (seen.has(id)) throw new Error(`Duplicate book perspective: ${id}`);
    seen.add(id);
    for (const [key, heading] of Object.entries(version.sectionKeys ?? {})) {
      if (!version.headings.includes(heading)) {
        throw new Error(
          `${id}: section key "${key}" refers to missing heading "${heading}"`,
        );
      }
    }
    const mapped = Object.values(version.sectionKeys ?? {});
    if (new Set(mapped).size !== mapped.length) {
      throw new Error(`${id}: a heading cannot have multiple section keys`);
    }
  }
  for (const version of versions) {
    if (!seen.has(`${version.chapter}:default`)) {
      throw new Error(
        `Book perspective has no default chapter: ${version.chapter}`,
      );
    }
  }
}
