import { createGeneratedContentReader } from '@/lib/generated-content/database';
import type {
  ChannelPointerRow,
  CliArtifactPayload,
  CliArtifactRow,
  PackageExportRow,
  ReferencePageRow,
  ReleaseRow,
} from '@/lib/generated-content/schemas';

export interface GeneratedReleaseSnapshot {
  channels: ChannelPointerRow['channel'][];
  cli: { payload: CliArtifactPayload; row: CliArtifactRow };
  packages: PackageExportRow[];
  pages: ReferencePageRow[];
  release: ReleaseRow;
  routeVersion: string;
}

export interface GeneratedReleaseSummary {
  channels: ChannelPointerRow['channel'][];
  release: ReleaseRow;
  routeVersion: string;
}

const snapshotPromises = new Map<
  string,
  Promise<GeneratedReleaseSnapshot | null>
>();
let releaseSummariesPromise: Promise<GeneratedReleaseSummary[]> | undefined;

export function canonicalVersionToRouteVersion(version: string): string {
  return `v${version}`;
}

export function routeVersionToCanonicalVersion(
  routeVersion: string,
): string | null {
  if (!routeVersion.startsWith('v') || routeVersion.length === 1) {
    return null;
  }
  return routeVersion.slice(1);
}

async function readReleaseSummaries(): Promise<GeneratedReleaseSummary[]> {
  const reader = createGeneratedContentReader();
  try {
    const [releases, channels] = await Promise.all([
      reader.listReleases(),
      reader.listChannels(),
    ]);
    return releases.map((release) => ({
      channels: channels
        .filter((pointer) => pointer.release_version === release.version)
        .map((pointer) => pointer.channel),
      release,
      routeVersion: canonicalVersionToRouteVersion(release.version),
    }));
  } finally {
    await reader.close();
  }
}

export function listGeneratedReleaseSummaries(): Promise<
  GeneratedReleaseSummary[]
> {
  releaseSummariesPromise ??= readReleaseSummaries();
  return releaseSummariesPromise;
}

export async function listGeneratedReleaseRouteVersions(): Promise<string[]> {
  return (await listGeneratedReleaseSummaries()).map(
    (release) => release.routeVersion,
  );
}

export function selectFeaturedGeneratedRelease(
  releases: readonly GeneratedReleaseSummary[],
): GeneratedReleaseSummary | null {
  for (const channel of ['stable', 'canary', 'nightly'] as const) {
    const release = releases.find((candidate) =>
      candidate.channels.includes(channel),
    );
    if (release) return release;
  }
  return releases[0] ?? null;
}

export function isPrereleaseVersion(version: string): boolean {
  return version.includes('-');
}

async function readSnapshot(
  routeVersion: string,
): Promise<GeneratedReleaseSnapshot | null> {
  const version = routeVersionToCanonicalVersion(routeVersion);
  if (!version) return null;

  const releaseSummary = (await listGeneratedReleaseSummaries()).find(
    (candidate) => candidate.release.version === version,
  );
  if (!releaseSummary) return null;

  const reader = createGeneratedContentReader();
  try {
    const [packages, pages, cli] = await Promise.all([
      reader.listPackageExports(version),
      reader.listReferencePages(version),
      reader.getCliArtifact(version),
    ]);
    if (!cli) return null;

    return {
      channels: releaseSummary.channels,
      cli,
      packages,
      pages,
      release: releaseSummary.release,
      routeVersion,
    };
  } finally {
    await reader.close();
  }
}

export function loadGeneratedReleaseSnapshot(
  routeVersion: string,
): Promise<GeneratedReleaseSnapshot | null> {
  const existing = snapshotPromises.get(routeVersion);
  if (existing) return existing;

  const pending = readSnapshot(routeVersion);
  snapshotPromises.set(routeVersion, pending);
  return pending;
}
