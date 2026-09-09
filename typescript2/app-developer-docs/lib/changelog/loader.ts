import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const canonicalTitle = '# Changelog';
const versionHeadingPattern = /^## \[([^\]]+)\](?:\([^)]*\))?(?:\s+-\s+.*)?$/;
const semanticVersionPattern =
  /^\d+\.\d+\.\d+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/;

export interface ChangelogEntry {
  id: string;
  version: string;
}

export interface CanonicalChangelog {
  entries: ChangelogEntry[];
  headingIds: Array<string | undefined>;
  markdown: string;
  sourcePath: string;
}

export function changelogVersionId(version: string) {
  const slug = version
    .toLowerCase()
    .replaceAll(/[^a-z0-9]+/g, '-')
    .replaceAll(/(^-|-$)/g, '');
  if (!slug) throw new Error(`Invalid changelog version: ${version}`);
  return `v${slug}`;
}

export function parseCanonicalChangelog(
  source: string,
  sourcePath: string,
): CanonicalChangelog {
  const normalizedSource = source.replaceAll('\r\n', '\n');
  const [firstLine, ...rest] = normalizedSource.split('\n');
  if (firstLine !== canonicalTitle) {
    throw new Error(`Unexpected canonical changelog heading: ${firstLine}`);
  }

  const markdown = rest.join('\n').trim();
  const entries: ChangelogEntry[] = [];
  const headingIds: Array<string | undefined> = [];

  for (const line of markdown.split('\n')) {
    if (!line.startsWith('## ')) continue;

    const match = line.match(versionHeadingPattern);
    if (!match) {
      if (line.startsWith('## [')) {
        throw new Error(`Malformed changelog version heading: ${line}`);
      }
      headingIds.push(undefined);
      continue;
    }

    const version = match[1];
    if (!semanticVersionPattern.test(version)) {
      throw new Error(`Invalid changelog version heading: ${line}`);
    }
    const id = changelogVersionId(version);
    entries.push({ id, version });
    headingIds.push(id);
  }

  if (entries.length === 0) {
    throw new Error('The canonical changelog has no version headings');
  }
  if (new Set(entries.map(({ id }) => id)).size !== entries.length) {
    throw new Error('The canonical changelog has duplicate version headings');
  }

  return { entries, headingIds, markdown, sourcePath };
}

export async function loadCanonicalChangelog(): Promise<CanonicalChangelog> {
  const sourcePath = resolve(process.cwd(), '..', '..', 'CHANGELOG.md');
  return parseCanonicalChangelog(
    await readFile(sourcePath, 'utf8'),
    sourcePath,
  );
}
