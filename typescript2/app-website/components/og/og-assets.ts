import { readFileSync } from 'node:fs';
import { join } from 'node:path';

// Shared asset loaders for the Open Graph / link-preview image renderer.
//
// Fonts and the lamb badge are read from disk relative to the project root.
// They live under `components/og/` (not `public/`, whose files are not bundled
// into serverless functions) and are force-included for the `/api/og` route
// via `outputFileTracingIncludes` in next.config so they survive on Vercel.
// Buffers are memoized per server instance.

export interface OgFont {
  name: string;
  data: Buffer;
  weight: 400 | 500 | 600;
  style: 'normal' | 'italic';
}

const OG_DIR = join(process.cwd(), 'components', 'og');

const read = (...segments: string[]) => readFileSync(join(OG_DIR, ...segments));

let fonts: OgFont[] | null = null;

export function ogFonts(): OgFont[] {
  if (!fonts) {
    fonts = [
      {
        data: read('fonts', 'InstrumentSerif-Regular.ttf'),
        name: 'Instrument Serif',
        style: 'normal',
        weight: 400,
      },
      {
        data: read('fonts', 'InstrumentSerif-Italic.ttf'),
        name: 'Instrument Serif',
        style: 'italic',
        weight: 400,
      },
      {
        data: read('fonts', 'IBMPlexMono-Regular.ttf'),
        name: 'IBM Plex Mono',
        style: 'normal',
        weight: 400,
      },
      {
        data: read('fonts', 'IBMPlexMono-Medium.ttf'),
        name: 'IBM Plex Mono',
        style: 'normal',
        weight: 500,
      },
      {
        data: read('fonts', 'IBMPlexMono-SemiBold.ttf'),
        name: 'IBM Plex Mono',
        style: 'normal',
        weight: 600,
      },
    ];
  }
  return fonts;
}

let lamb: string | null = null;

/** The ink lamb mark as a base64 data URI (rendered inside the cream badge). */
export function lambDataUri(): string {
  if (!lamb) {
    lamb = `data:image/png;base64,${read('lamb-mark.png').toString('base64')}`;
  }
  return lamb;
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

/** The team photo, as a data URI (for the /who-are-we card). */
export function teamPhoto(): string {
  return imageDataUri('team.jpg');
}
