import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { generateAnnotationAssets } from '../lib/snippets/annotation-assets';
import { loadAnnotationSources } from '../lib/snippets/annotation-sources';

const sources = await loadAnnotationSources();
const directory = resolve('public/book/annotations');
await mkdir(directory, { recursive: true });
const records = await Promise.all(
  sources.map(async (source) => {
    const { images, metadata } = await generateAnnotationAssets(source);
    await Promise.all(
      images.map((image) =>
        writeFile(
          resolve(directory, `${source.id}-${image.theme}.svg`),
          image.svg,
        ),
      ),
    );
    return [source.id, metadata] as const;
  }),
);
await writeFile(
  resolve('lib/snippets/annotation-manifest.json'),
  `${JSON.stringify(Object.fromEntries(records), null, 2)}\n`,
);
await writeFile(
  resolve(directory, 'FONT-LICENSE.txt'),
  await readFile(resolve('node_modules/geist/LICENSE.TXT')),
);
console.log(
  `Generated ${records.length * 2} annotated SVGs from ${records.length} source snippets.`,
);
