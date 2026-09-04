import {
  type GeneratedReleaseSnapshot,
  type GeneratedReleaseSummary,
  isPrereleaseVersion,
  listGeneratedReleaseSummaries,
  loadGeneratedReleaseSnapshot,
} from '@/lib/generated-content/build-content';
import {
  findCliCommand,
  flattenCliCommands,
} from '@/lib/generated-content/cli-routes';
import type { GeneratedSearchIndex, SearchEntry } from '@/lib/search';

function channelSuffix(channels: readonly string[]): string {
  return channels.length > 0 ? ` · ${channels.join(', ')}` : '';
}

export interface GeneratedVersionOption {
  channels: string[];
  href: string;
  routeVersion: string;
}

type VersionDestination =
  | { kind: 'cli-command'; commandPath: readonly string[] }
  | { kind: 'cli-command-index' }
  | { kind: 'cli-overview' }
  | { kind: 'package'; routePath?: string };

export async function listGeneratedVersionOptions(
  destination: VersionDestination,
): Promise<GeneratedVersionOption[]> {
  const releases = await listGeneratedReleaseSummaries();
  const options: GeneratedVersionOption[] = [];

  for (const release of releases) {
    const snapshot = await loadGeneratedReleaseSnapshot(release.routeVersion);
    if (!snapshot) continue;
    let suffix = '';
    if (destination.kind === 'package') {
      if (
        destination.routePath &&
        !snapshot.pages.some(
          (page) => page.route_path === destination.routePath,
        )
      ) {
        continue;
      }
      suffix = destination.routePath ? `/${destination.routePath}` : '';
    } else if (destination.kind === 'cli-command') {
      if (!findCliCommand(snapshot.cli.payload.root, destination.commandPath)) {
        continue;
      }
      suffix = `/commands/${destination.commandPath.join('/')}`;
    } else if (destination.kind === 'cli-command-index') {
      suffix = '/commands';
    }
    const root = destination.kind === 'package' ? '/baml/packages' : '/cli';
    options.push({
      channels: release.channels,
      href: `${root}/${release.routeVersion}${suffix}`,
      routeVersion: release.routeVersion,
    });
  }

  return options;
}

export function generatedRoutePaths(
  snapshot: GeneratedReleaseSnapshot,
): string[] {
  const packageRoot = `/baml/packages/${snapshot.routeVersion}`;
  const cliRoot = `/cli/${snapshot.routeVersion}`;
  return [
    packageRoot,
    ...snapshot.pages.map((page) => `${packageRoot}/${page.route_path}`),
    cliRoot,
    `${cliRoot}/commands`,
    ...flattenCliCommands(snapshot.cli.payload.root).map(
      (command) => `${cliRoot}/commands/${command.command_path.join('/')}`,
    ),
  ];
}

export function generatedSearchEntries(
  release: GeneratedReleaseSummary,
  snapshot: GeneratedReleaseSnapshot,
): SearchEntry[] {
  const entries: SearchEntry[] = [];
  const current = release.channels.length > 0;
  const version = release.routeVersion;
  const packageGroup = `Standard packages · ${version}${channelSuffix(release.channels)}`;
  const cliGroup = `CLI · ${version}${channelSuffix(release.channels)}`;
  const packageRoot = `/baml/packages/${version}`;
  const cliRoot = `/cli/${version}`;

  entries.push(
    {
      current,
      group: packageGroup,
      href: packageRoot,
      label: `Standard packages ${version}`,
      version,
    },
    {
      current,
      group: cliGroup,
      href: cliRoot,
      label: `BAML CLI ${version}`,
      version,
    },
    {
      current,
      group: cliGroup,
      href: `${cliRoot}/commands`,
      label: `Command index ${version}`,
      version,
    },
  );

  for (const page of snapshot.pages) {
    const href = `${packageRoot}/${page.route_path}`;
    entries.push({
      current,
      group: packageGroup,
      href,
      keywords: `${page.page_kind} ${page.page_data.summary ?? ''}`,
      label: page.qualified_name,
      version,
    });
    if ('member_anchors' in page.page_data) {
      for (const member of page.page_data.member_anchors) {
        entries.push({
          current,
          group: packageGroup,
          href: `${href}#${member.anchor}`,
          keywords: member.member_kind,
          label: `${page.qualified_name}.${member.label}`,
          version,
        });
      }
    }
  }

  for (const command of flattenCliCommands(snapshot.cli.payload.root)) {
    entries.push({
      current,
      group: cliGroup,
      href: `${cliRoot}/commands/${command.command_path.join('/')}`,
      keywords: command.description ?? '',
      label: `baml ${command.command_path.join(' ')}`,
      version,
    });
  }

  return entries;
}

export async function listGeneratedSitemapRoutes(): Promise<
  { lastModified: Date; path: string }[]
> {
  const releases = await listGeneratedReleaseSummaries();
  const routes: { lastModified: Date; path: string }[] = [];

  for (const release of releases) {
    if (isPrereleaseVersion(release.release.version)) continue;
    const snapshot = await loadGeneratedReleaseSnapshot(release.routeVersion);
    if (!snapshot) continue;
    routes.push(
      ...generatedRoutePaths(snapshot).map((path) => ({
        lastModified: release.release.released_at,
        path,
      })),
    );
  }

  return routes;
}

export async function buildGeneratedSearchIndex(): Promise<GeneratedSearchIndex> {
  const releases = await listGeneratedReleaseSummaries();
  const entries: SearchEntry[] = [];

  for (const release of releases) {
    const snapshot = await loadGeneratedReleaseSnapshot(release.routeVersion);
    if (!snapshot) continue;
    entries.push(...generatedSearchEntries(release, snapshot));
  }

  return {
    entries,
    versions: releases.map((release) => ({
      channels: release.channels,
      current: release.channels.length > 0,
      routeVersion: release.routeVersion,
    })),
  };
}
