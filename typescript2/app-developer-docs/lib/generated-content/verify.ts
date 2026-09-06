import {
  listAllStoredRoutes,
  listDocumentReleaseSummaries,
} from '@/lib/generated-content/document-store';

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

  const paths = new Set(routes.map((route) => route.path));
  for (const route of routes) {
    if (route.route_metadata.canonicalVersion !== version) {
      throw new Error(`Route ${route.path} has mismatched version metadata.`);
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
