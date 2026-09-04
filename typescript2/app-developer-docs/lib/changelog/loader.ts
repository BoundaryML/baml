import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const title = '# BAML Language changelog';
const versionHeadingPattern = /^## \[([^\]]+)\](?:\([^)]*\))?(?:\s+-\s+.*)?$/gm;

export interface ChangelogEntry {
  id: string;
  version: string;
}

export interface CanonicalChangelog {
  entries: ChangelogEntry[];
  markdown: string;
  sourcePath: string;
}

export function changelogVersionId(version: string) {
  return `v${version.replaceAll('.', '-')}`;
}

export async function loadCanonicalChangelog(): Promise<CanonicalChangelog> {
  const sourcePath = resolve(
    process.cwd(),
    '..',
    '..',
    'baml_language',
    'CHANGELOG.md',
  );
  const source = (await readFile(sourcePath, 'utf8')).replaceAll('\r\n', '\n');
  const [firstLine, ...rest] = source.split('\n');
  if (firstLine !== title) {
    throw new Error(`Unexpected canonical changelog heading: ${firstLine}`);
  }

  const markdown = rest.join('\n').trim();
  const entries = [...markdown.matchAll(versionHeadingPattern)].map(
    (match) => ({
      id: changelogVersionId(match[1]),
      version: match[1],
    }),
  );
  if (entries.length === 0) {
    throw new Error('The canonical changelog has no version headings');
  }
  if (new Set(entries.map(({ id }) => id)).size !== entries.length) {
    throw new Error('The canonical changelog has duplicate version headings');
  }

  return { entries, markdown, sourcePath };
}
