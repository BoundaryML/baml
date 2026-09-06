import { hashDocumentManifest } from '@/lib/generated-content/document-ir';
import {
  listAllStoredRoutes,
  listDocumentReleaseSummaries,
} from '@/lib/generated-content/document-store';
import { canonicalVersionToRouteVersion } from '@/lib/generated-content/versions';

export interface ReleaseVerificationSummary {
  cli_commands: number;
  content_schema_version: number;
  manifest_hash: string;
  reference_pages: number;
  routes: number;
  unique_snapshots: number;
  version: string;
}

export async function verifyGeneratedRelease(
  version: string,
): Promise<ReleaseVerificationSummary> {
  const release = (await listDocumentReleaseSummaries()).find(
    (candidate) => candidate.release.version === version,
  );
  if (!release) {
    throw new Error(`Generated-content release ${version} does not exist.`);
  }

  const routes = (await listAllStoredRoutes()).filter(
    (route) => route.version === version,
  );
  if (routes.length !== release.release.route_count) {
    throw new Error(
      `Release ${version} route count does not match its manifest.`,
    );
  }
  const uniqueSnapshots = new Set(routes.map((route) => route.content_hash));
  if (uniqueSnapshots.size !== release.release.unique_snapshot_count) {
    throw new Error(
      `Release ${version} snapshot count does not match its manifest.`,
    );
  }

  const actualManifestHash = hashDocumentManifest({
    contentSchemaVersion: release.release.content_schema_version,
    routes: routes.map((route) => ({
      contentHash: route.content_hash,
      metadata: route.route_metadata,
      path: route.path,
    })),
    sourceRevision: release.release.source_revision,
    version,
  });
  if (actualManifestHash !== release.release.manifest_hash) {
    throw new Error(
      `Release ${version} manifest hash does not match its routes.`,
    );
  }

  const paths = new Set(routes.map((route) => route.path));
  const requiredRoutes = new Map([
    ['baml/packages', 'packageIndex'],
    ['cli', 'cliOverview'],
    ['cli/commands', 'cliCommandIndex'],
  ]);
  for (const [path, kind] of requiredRoutes) {
    const route = routes.find((candidate) => candidate.path === path);
    if (!route || route.route_metadata.kind !== kind) {
      throw new Error(`Release ${version} is missing required route ${path}.`);
    }
  }

  const routeVersion = canonicalVersionToRouteVersion(version);
  for (const route of routes) {
    if (route.route_metadata.canonicalVersion !== version) {
      throw new Error(`Route ${route.path} has mismatched version metadata.`);
    }
    if (route.route_metadata.routeVersion !== routeVersion) {
      throw new Error(`Route ${route.path} has mismatched route version.`);
    }
    const isPackageRoute =
      route.path === 'baml/packages' || route.path.startsWith('baml/packages/');
    const isCliRoute = route.path === 'cli' || route.path.startsWith('cli/');
    const pathSuffix = isPackageRoute
      ? route.path.slice('baml/packages'.length)
      : isCliRoute
        ? route.path.slice('cli'.length)
        : null;
    if (pathSuffix === null) {
      throw new Error(`Route ${route.path} is outside the public contract.`);
    }
    const publicRoot = isPackageRoute ? '/baml/packages' : '/cli';
    const expectedPublicPath = `${publicRoot}/${routeVersion}${pathSuffix}`;
    if (route.route_metadata.publicPath !== expectedPublicPath) {
      throw new Error(`Route ${route.path} has mismatched public path.`);
    }
    for (const block of route.content.blocks) {
      if (block.type !== 'bamlReference') continue;
      const pageLinks =
        block.page.page_kind === 'package' ||
        block.page.page_kind === 'namespace'
          ? block.page.children
          : block.page.cross_references;
      for (const link of [...pageLinks, ...block.namespacedChildren]) {
        if (!paths.has(`baml/packages/${link.route_path}`)) {
          throw new Error(
            `Route ${route.path} links to missing document ${link.route_path}.`,
          );
        }
      }
    }
  }

  return {
    cli_commands: routes.filter(
      (route) => route.route_metadata.kind === 'cliCommand',
    ).length,
    content_schema_version: release.release.content_schema_version,
    manifest_hash: release.release.manifest_hash,
    reference_pages: routes.filter(
      (route) => route.route_metadata.kind === 'packageReference',
    ).length,
    routes: routes.length,
    unique_snapshots: uniqueSnapshots.size,
    version,
  };
}
