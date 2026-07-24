// Server-rendered JSON-LD structured data. These components emit
// <script type="application/ld+json"> tags into the SSR HTML so search engines
// and rich-result crawlers can read Organization, WebSite, Article, and
// SoftwareApplication metadata. Keep this a server component (no 'use client').

// Resolve the public base URL the same way app/_lib/metadata.ts and the root
// layout do, so canonical @id values match the rest of the site's metadata.
const SITE_URL =
  process.env.NEXT_PUBLIC_BASE_URL ??
  (process.env.VERCEL_PROJECT_PRODUCTION_URL
    ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
    : 'http://localhost:3000');

// Stable @id anchors so the graph nodes can reference each other.
const ORGANIZATION_ID = `${SITE_URL}/#organization`;
const WEBSITE_ID = `${SITE_URL}/#website`;

// Real social profiles, sourced from app/_lib/config.tsx (siteConfig).
const SAME_AS = [
  'https://github.com/boundaryml',
  'https://twitter.com/boundaryml',
  'https://boundaryml.com/discord',
  'https://linkedin.com/company/boundaryml',
  'https://youtube.com/@boundaryml',
];

function toAbsoluteUrl(pathOrUrl: string): string {
  if (/^https?:\/\//.test(pathOrUrl)) return pathOrUrl;
  return `${SITE_URL}${pathOrUrl.startsWith('/') ? '' : '/'}${pathOrUrl}`;
}

// Normalize a date (e.g. "Jan 24, 2025" or an ISO string) to ISO 8601, which is
// what schema.org expects. Falls back to the original if it can't be parsed.
function toIsoDate(value: string): string {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toISOString();
}

/** Renders a single JSON-LD blob as a server-side <script> tag. */
function JsonLd({ data }: { data: Record<string, unknown> }) {
  // JSON-LD must be serialized into a raw <script> tag; the payload is our own
  // static data, so the dangerouslySetInnerHTML lint here is expected.
  return (
    <script
      dangerouslySetInnerHTML={{ __html: JSON.stringify(data) }}
      type="application/ld+json"
    />
  );
}

/**
 * Organization + WebSite graph, rendered once on every page via the root
 * layout. The WebSite node references the Organization as its publisher.
 */
export function SiteStructuredData() {
  const data = {
    '@context': 'https://schema.org',
    '@graph': [
      {
        '@id': ORGANIZATION_ID,
        '@type': 'Organization',
        legalName: 'Gloo Chat, Inc.',
        logo: `${SITE_URL}/baml-sheep.png`,
        name: 'BAML',
        sameAs: SAME_AS,
        url: `${SITE_URL}/`,
      },
      {
        '@id': WEBSITE_ID,
        '@type': 'WebSite',
        name: 'BAML',
        publisher: { '@id': ORGANIZATION_ID },
        url: `${SITE_URL}/`,
      },
    ],
  };

  return <JsonLd data={data} />;
}

/**
 * Article graph for a blog post or podcast episode. `author` falls back to the
 * Organization when the source has no named author (e.g. podcast episodes).
 */
export function ArticleStructuredData({
  headline,
  description,
  datePublished,
  dateModified,
  authorName,
  image,
  url,
}: {
  headline: string;
  description: string;
  datePublished: string;
  dateModified?: string;
  authorName?: string;
  image: string;
  url: string;
}) {
  const canonicalUrl = toAbsoluteUrl(url);
  const data = {
    '@context': 'https://schema.org',
    '@type': 'Article',
    author: authorName
      ? { '@type': 'Person', name: authorName }
      : { '@id': ORGANIZATION_ID, '@type': 'Organization', name: 'BAML' },
    datePublished: toIsoDate(datePublished),
    description,
    headline,
    image: toAbsoluteUrl(image),
    mainEntityOfPage: { '@id': canonicalUrl, '@type': 'WebPage' },
    publisher: {
      '@id': ORGANIZATION_ID,
      '@type': 'Organization',
      logo: {
        '@type': 'ImageObject',
        url: `${SITE_URL}/baml-sheep.png`,
      },
      name: 'BAML',
    },
    url: canonicalUrl,
    ...(dateModified ? { dateModified: toIsoDate(dateModified) } : {}),
  };

  return <JsonLd data={data} />;
}

/**
 * SoftwareApplication + free Offer for /pricing, backing the "free and open
 * source" positioning.
 */
export function SoftwareApplicationStructuredData() {
  const data = {
    '@context': 'https://schema.org',
    '@type': 'SoftwareApplication',
    applicationCategory: 'DeveloperApplication',
    name: 'BAML',
    offers: {
      '@type': 'Offer',
      price: '0',
      priceCurrency: 'USD',
    },
    operatingSystem: 'macOS, Windows, Linux',
    publisher: { '@id': ORGANIZATION_ID },
    url: `${SITE_URL}/`,
  };

  return <JsonLd data={data} />;
}
