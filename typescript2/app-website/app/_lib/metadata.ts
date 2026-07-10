import type { Metadata } from 'next';

// Central helper for on-brand link previews. Every page builds its metadata
// through `createMetadata`, which wires a complete Open Graph + Twitter card
// pointing at the shared `/api/og` renderer so each page gets a branded
// "cream + purple rail" preview image with its own headline and kicker.

// Resolve the public base URL from the deployment's own production domain
// (e.g. new.boundaryml.com) rather than hardcoding a host — otherwise og:image
// and canonical URLs point at a different site than the one being served.
const SITE_URL =
  process.env.NEXT_PUBLIC_BASE_URL ??
  (process.env.VERCEL_PROJECT_PRODUCTION_URL
    ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
    : 'http://localhost:3000');

export const TWITTER_HANDLE = '@boundaryml';

/** Build the `/api/og` URL that renders a page's preview image. */
export function ogImagePath(opts: {
  title: string;
  eyebrow?: string;
  /** Card subtitle (mono, under the headline). */
  description?: string;
  /** Render the horizontal computing-eras timeline. */
  timeline?: boolean;
  /** Render the podcast host avatars. */
  podcast?: boolean;
  /** Render the team photo. */
  team?: boolean;
}): string {
  const search = new URLSearchParams();
  search.set('title', opts.title);
  if (opts.eyebrow) search.set('eyebrow', opts.eyebrow);
  if (opts.description) search.set('desc', opts.description);
  if (opts.timeline) search.set('timeline', '1');
  if (opts.podcast) search.set('podcast', '1');
  if (opts.team) search.set('team', '1');
  return `/api/og?${search.toString()}`;
}

export interface CreateMetadataOptions {
  /** Page headline — used for <title> (templated to "… | BAML"), OG and the preview image. */
  title: string;
  description: string;
  /** Path for canonical + og:url, e.g. "/quickstart". Defaults to "/". */
  path?: string;
  /** Deprecated: the card no longer renders a kicker. Ignored. */
  eyebrow?: string;
  /** Override the OG/Twitter/image title when it should differ from <title>. */
  ogTitle?: string;
  /** Card subtitle (mono, under the headline). Use only for a real fact. */
  ogSubtitle?: string;
  /** Card variant: the computing-eras timeline (home + /explore). */
  timeline?: boolean;
  /** Card variant: podcast host avatars (/podcast). */
  podcast?: boolean;
  /** Card variant: the team photo (/who-are-we). */
  team?: boolean;
  /** Set an exact <title>, bypassing the "… | BAML" template. */
  titleAbsolute?: string;
  /** Provide a custom preview image instead of the generated one. */
  image?: string;
  type?: 'website' | 'article';
  keywords?: string | string[];
  /** Set false for noindex pages (members-only, etc.). */
  indexable?: boolean;
}

export function createMetadata({
  title,
  description,
  path = '/',
  ogTitle,
  ogSubtitle,
  titleAbsolute,
  image,
  type = 'website',
  keywords,
  indexable = true,
  timeline,
  podcast,
  team,
}: CreateMetadataOptions): Metadata {
  const normalizedPath = path.startsWith('/') ? path : `/${path}`;
  const url = `${SITE_URL}${normalizedPath}`;
  const cardTitle = ogTitle ?? title;
  const ogImage =
    image ??
    ogImagePath({
      description: ogSubtitle,
      podcast,
      team,
      timeline,
      title: cardTitle,
    });

  return {
    alternates: { canonical: url },
    description,
    keywords,
    openGraph: {
      description,
      images: [{ alt: cardTitle, height: 630, url: ogImage, width: 1200 }],
      locale: 'en_US',
      siteName: 'BAML',
      title: cardTitle,
      type,
      url,
    },
    title: titleAbsolute ? { absolute: titleAbsolute } : title,
    twitter: {
      card: 'summary_large_image',
      creator: TWITTER_HANDLE,
      description,
      images: [ogImage],
      site: TWITTER_HANDLE,
      title: cardTitle,
    },
    ...(indexable ? {} : { robots: { follow: false, index: false } }),
  };
}
