export const markdownPageSlugs = [
  'index',
  'quickstart',
  'explore',
  'pricing',
  'changelog',
] as const;

export type MarkdownPageSlug = (typeof markdownPageSlugs)[number];

export const markdownCanonicalPaths: Record<MarkdownPageSlug, string> = {
  index: '/',
  quickstart: '/quickstart',
  explore: '/explore',
  pricing: '/pricing',
  changelog: '/changelog',
};

export function isMarkdownPageSlug(value: string): value is MarkdownPageSlug {
  return markdownPageSlugs.includes(value as MarkdownPageSlug);
}
