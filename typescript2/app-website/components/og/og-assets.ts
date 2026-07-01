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
