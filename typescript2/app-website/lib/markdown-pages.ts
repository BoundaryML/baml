export const markdownPageSlugs = [
  'index',
  'quickstart',
  'explore',
  'pricing',
] as const;

export type MarkdownPageSlug = (typeof markdownPageSlugs)[number];

export const markdownCanonicalPaths: Record<MarkdownPageSlug, string> = {
  explore: '/explore',
  index: '/',
  pricing: '/pricing',
  quickstart: '/quickstart',
};

export function isMarkdownPageSlug(value: string): value is MarkdownPageSlug {
  return markdownPageSlugs.includes(value as MarkdownPageSlug);
}
