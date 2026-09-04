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

const snapshotPromises = new Map<
  string,
  Promise<GeneratedReleaseSnapshot | null>
>();

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

export async function listGeneratedReleaseRouteVersions(): Promise<string[]> {
  const reader = createGeneratedContentReader();
  try {
    const releases = await reader.listReleases();
    return releases.map((release) =>
      canonicalVersionToRouteVersion(release.version),
    );
  } finally {
    await reader.close();
  }
}

async function readSnapshot(
  routeVersion: string,
): Promise<GeneratedReleaseSnapshot | null> {
  const version = routeVersionToCanonicalVersion(routeVersion);
  if (!version) return null;

  const reader = createGeneratedContentReader();
  try {
    const [releases, channels, packages, pages, cli] = await Promise.all([
      reader.listReleases(),
      reader.listChannels(),
      reader.listPackageExports(version),
      reader.listReferencePages(version),
      reader.getCliArtifact(version),
    ]);
    const release = releases.find((candidate) => candidate.version === version);
    if (!release || !cli) return null;

    return {
      channels: channels
        .filter((pointer) => pointer.release_version === version)
        .map((pointer) => pointer.channel),
      cli,
      packages,
      pages,
      release,
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
