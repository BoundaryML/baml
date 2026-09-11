import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import type { AnnotationSource } from './annotation-sources';
import { annotationSpecs } from './annotation-specs';
import { renderAnnotatedSnippet } from './annotation-svg';
import { highlightCode } from './highlighter';

let embeddedFonts: Promise<string> | undefined;
function fontStyles(): Promise<string> {
  embeddedFonts ??= Promise.all([
    readFile(
      resolve(
        'node_modules/geist/dist/fonts/geist-mono/GeistMono-Regular.woff2',
      ),
    ),
    readFile(
      resolve('node_modules/geist/dist/fonts/geist-sans/Geist-Regular.woff2'),
    ),
  ]).then(
    ([mono, sans]) =>
      `@font-face{font-family:AnnotationMono;src:url(data:font/woff2;base64,${mono.toString('base64')}) format('woff2')}@font-face{font-family:AnnotationSans;src:url(data:font/woff2;base64,${sans.toString('base64')}) format('woff2')}.annotation-label{font-family:AnnotationSans,sans-serif}`,
  );
  return embeddedFonts;
}

export function annotationSourceHash(code: string): string {
  return createHash('sha256').update(code).digest('hex');
}

export async function generateAnnotationAssets(source: AnnotationSource) {
  const tokens = await highlightCode(source.code, source.language);
  const fontCss = await fontStyles();
  const annotations = annotationSpecs[source.id];
  const images = (['light', 'dark'] as const).map((theme) => {
    const result = renderAnnotatedSnippet({
      annotations,
      charWidth: 12,
      code: source.code,
      fontFamily: 'AnnotationMono',
      theme,
      tokens: tokens[theme],
    });
    // The site supplies the single editor frame around the image and its controls.
    const svg = result.svg
      .replace('</style>', `${fontCss}</style>`)
      .replace(/<rect [^>]+\/>/, (rect) =>
        rect.replace('rx="12"', 'rx="0"').replace(/ stroke="[^"]+"/, ''),
      );
    return { ...result, svg, theme };
  });
  return {
    images,
    metadata: {
      description: annotations
        .map((item) => `${item.label}: ${item.text.trim()}`)
        .join('. '),
      height: images[0].height,
      sourceHash: annotationSourceHash(source.code),
      width: images[0].width,
    },
  };
}
