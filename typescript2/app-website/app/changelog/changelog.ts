const RELEASE_HEADER =
  /^## \[([^\]]+)\]\(([^)]+)\) - (\d{4}-\d{2}-\d{2})\s*$/gm;

export interface ChangelogRelease {
  body: string;
  compareUrl: string;
  date: string;
  version: string;
}

export function parseChangelog(source: string): ChangelogRelease[] {
  const headers = [...source.matchAll(RELEASE_HEADER)];

  return headers.map((header, index) => {
    const bodyStart = (header.index ?? 0) + header[0].length;
    const bodyEnd = headers[index + 1]?.index ?? source.length;

    return {
      body: source.slice(bodyStart, bodyEnd).trim(),
      compareUrl: header[2],
      date: header[3],
      version: header[1],
    };
  });
}

export function releaseId(version: string): string {
  return `release-${version.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;
}

export function changeCount(body: string): number {
  return body.match(/^- /gm)?.length ?? 0;
}
