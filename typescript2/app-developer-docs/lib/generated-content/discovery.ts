import {
  listDocumentReleaseSummaries,
  listDocumentVersionOptions,
  listStoredRouteIndex,
} from '@/lib/generated-content/document-store';
import { isPrereleaseVersion } from '@/lib/generated-content/versions';
import type { GeneratedSearchIndex, SearchEntry } from '@/lib/search';

function channelSuffix(aliases: readonly string[]): string {
  return aliases.length > 0 ? ` · ${aliases.join(', ')}` : '';
}

export interface GeneratedVersionOption {
  channels: string[];
  href: string;
  routeVersion: string;
}

export async function listGeneratedVersionOptions(
  path: string,
): Promise<GeneratedVersionOption[]> {
  return (await listDocumentVersionOptions(path)).map((option) => ({
    channels: option.aliases,
    href: option.href,
    routeVersion: option.routeVersion,
  }));
}

export async function listGeneratedSitemapRoutes(): Promise<
  { lastModified: Date; path: string }[]
> {
  const routes = await listStoredRouteIndex();
  return routes
    .filter((route) => !isPrereleaseVersion(route.version))
    .map((route) => ({
      lastModified: new Date(route.route_metadata.releasedAt),
      path: route.route_metadata.publicPath,
    }));
}

export async function buildGeneratedSearchIndex(): Promise<GeneratedSearchIndex> {
  const [releases, routes] = await Promise.all([
    listDocumentReleaseSummaries(),
    listStoredRouteIndex(),
  ]);
  const aliasesByVersion = new Map(
    releases.map((release) => [release.release.version, release.aliases]),
  );
  const entries: SearchEntry[] = [];

  for (const route of routes) {
    const aliases = aliasesByVersion.get(route.version) ?? [];
    const current = aliases.length > 0;
    const group = `${
      route.route_metadata.surface === 'packages' ? 'Standard packages' : 'CLI'
    } · ${route.route_metadata.routeVersion}${channelSuffix(aliases)}`;
    for (const entry of route.route_metadata.searchEntries) {
      entries.push({
        current,
        group,
        href: `${route.route_metadata.publicPath}${entry.anchor ? `#${entry.anchor}` : ''}`,
        keywords: entry.keywords,
        label: entry.label,
        version: route.route_metadata.routeVersion,
      });
    }
  }

  return {
    entries,
    versions: releases.map((release) => ({
      channels: release.aliases,
      current: release.aliases.length > 0,
      routeVersion: release.routeVersion,
    })),
  };
}
