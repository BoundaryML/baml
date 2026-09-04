export const primaryNavigation = [
  { href: '/', label: 'Home' },
  { href: '/baml', label: 'BAML' },
  { href: '/cli', label: 'CLI' },
  { href: '/bcs', label: 'Cloud' },
  { href: '/tutorials', label: 'Tutorials' },
  { href: '/examples', label: 'Examples' },
] as const;

export interface DocumentationLink {
  children?: DocumentationLink[];
  href: string;
  label: string;
}

interface DocumentationGroup {
  label: string;
  links: DocumentationLink[];
}

export const documentationNavigation: DocumentationGroup[] = [
  {
    label: 'BAML',
    links: [
      { href: '/baml', label: 'Overview' },
      { href: '/baml/get-started', label: 'Get started' },
      {
        children: [
          {
            children: [
              {
                href: '/baml/book/foundations/functions',
                label: 'Functions',
              },
            ],
            href: '/baml/book/foundations',
            label: 'Foundations',
          },
        ],
        href: '/baml/book',
        label: 'Book',
      },
      {
        children: [{ href: '/baml/language/functions', label: 'Functions' }],
        href: '/baml/language',
        label: 'Language reference',
      },
      { href: '/baml/packages', label: 'Standard packages' },
      {
        children: [{ href: '/baml/bridges/typescript', label: 'TypeScript' }],
        href: '/baml/bridges',
        label: 'Language bridges',
      },
    ],
  },
  {
    label: 'Products',
    links: [
      { href: '/cli', label: 'BAML CLI' },
      { href: '/bcs', label: 'Cloud' },
    ],
  },
  {
    label: 'Resources',
    links: [
      {
        children: [
          {
            href: '/tutorials/structured-extraction',
            label: 'Structured extraction',
          },
        ],
        href: '/tutorials',
        label: 'Tutorials',
      },
      {
        children: [
          {
            href: '/examples/classify-support-tickets',
            label: 'Classify support tickets',
          },
        ],
        href: '/examples',
        label: 'Examples',
      },
    ],
  },
];

export function flattenDocumentationLinks(
  links: DocumentationLink[],
): DocumentationLink[] {
  return links.flatMap((link) => [
    link,
    ...flattenDocumentationLinks(link.children ?? []),
  ]);
}

export const flattenedDocumentationNavigation = documentationNavigation.flatMap(
  (group) => flattenDocumentationLinks(group.links),
);

export const documentationPages: Array<{ href: string; label: string }> = [
  { href: '/', label: 'Home' },
  ...flattenedDocumentationNavigation.map(({ href, label }) => ({
    href,
    label,
  })),
];

export const searchablePages = documentationNavigation.flatMap((group) =>
  flattenDocumentationLinks(group.links).map(({ href, label }) => ({
    group: group.label,
    href,
    label,
  })),
);
