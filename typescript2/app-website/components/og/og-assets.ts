import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { ALT_LOGO } from '@/components/home/boundary-alt';

// Shared asset loaders for the Open Graph / link-preview image renderer.
//
// Fonts and images are read from disk relative to the project root.
// They live under `components/og/` (not `public/`, whose files are not bundled
// into serverless functions) and are force-included for the `/api/og` route
// via `outputFileTracingIncludes` in next.config so they survive on Vercel.
// Buffers are memoized per server instance.

export interface OgFont {
  name: string;
  data: Buffer;
  weight: 400 | 500;
  style: 'normal';
}

const OG_DIR = join(process.cwd(), 'components', 'og');

const read = (...segments: string[]) => readFileSync(join(OG_DIR, ...segments));

let fonts: OgFont[] | null = null;

export function ogFonts(): OgFont[] {
  if (!fonts) {
    fonts = [
      {
        data: read('fonts', 'Lexend-Medium.ttf'),
        name: 'Lexend',
        style: 'normal',
        weight: 500,
      },
      {
        data: read('fonts', 'Geist-Regular.ttf'),
        name: 'Geist',
        style: 'normal',
        weight: 400,
      },
    ];
  }
  return fonts;
}

const imgCache = new Map<string, string>();

/** Read a vendored image (in components/og/) as a base64 data URI, memoized. */
function imageDataUri(file: string, mime = 'image/jpeg'): string {
  let uri = imgCache.get(file);
  if (!uri) {
    uri = `data:${mime};base64,${read(file).toString('base64')}`;
    imgCache.set(file, uri);
  }
  return uri;
}

/** The "ai that works" podcast hosts' X avatars, as data URIs. */
export function podcastHosts(): { src: string; handle: string }[] {
  return [
    { handle: '@vaibcode', src: imageDataUri('host-vaibcode.jpg') },
    { handle: '@dexhorthy', src: imageDataUri('host-dexhorthy.jpg') },
  ];
}

// The site's Boundary logo (components/home/boundary-logo.tsx): the alt mark and the wordmark.
/** The logo's width over its height, for sizing it on the card. */
export const LOGO_ASPECT = ALT_LOGO.width / ALT_LOGO.height;

/** The white Boundary logo, as an SVG data URI. */
export function boundaryLogo(): string {
  const paths = ALT_LOGO.paths.map((d) => `<path d="${d}"/>`).join('');
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${ALT_LOGO.viewBox}" fill="#FEFEFE">${paths}</svg>`;
  return `data:image/svg+xml;base64,${Buffer.from(svg).toString('base64')}`;
}

/** The painting beside most cards, as a data URI. */
export function duskField(): string {
  return imageDataUri('dusk-field.jpg');
}

/** The team photo, as a data URI (for the /who-are-we card). */
export function teamPhoto(): string {
  return imageDataUri('team.jpg');
}
