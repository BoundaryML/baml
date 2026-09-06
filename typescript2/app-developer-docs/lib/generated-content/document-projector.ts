import { flattenCliCommands } from '@/lib/generated-content/cli-routes';
import { DOCUMENT_SCHEMA_VERSION } from '@/lib/generated-content/constants';
import {
  type DocumentReleaseBundle,
  type DocumentRouteInput,
  type DocumentRouteMetadata,
  type DocumentSearchEntry,
  type DocumentSnapshot,
  documentRouteMetadataSchema,
  documentSnapshotSchema,
  hashDocumentSnapshot,
} from '@/lib/generated-content/document-ir';
import {
  canonicalJson,
  jsonValueSchema,
  sha256,
} from '@/lib/generated-content/json';
import { exportedItemSchema } from '@/lib/generated-content/package-export';
import { declarationMemberGroups } from '@/lib/generated-content/reference-rendering';
import type { CompleteReleasePublicationInput } from '@/lib/generated-content/release-generator';
import type {
  CliCommandNodeInput,
  ReferencePageData,
} from '@/lib/generated-content/schemas';
import { canonicalVersionToRouteVersion } from '@/lib/generated-content/versions';

function snapshotSearchableText(snapshot: DocumentSnapshot): string {
  return JSON.stringify(snapshot)
    .replaceAll(/[{}[\]",:]/g, ' ')
    .replaceAll(/\s+/g, ' ')
    .trim();
}

function baseMetadata(
  release: CompleteReleasePublicationInput,
  input: {
    description: string;
    publicPath: string;
    searchEntries: DocumentSearchEntry[];
    title: string;
  },
) {
  return {
    canonicalVersion: release.version,
    description: input.description,
    generatorVersion: release.generatorVersion,
    publicPath: input.publicPath,
    releasedAt: release.releasedAt,
    routeVersion: canonicalVersionToRouteVersion(release.version),
    searchEntries: input.searchEntries,
    sourceRevision: release.sourceCommit,
    title: input.title,
    wrapperVersion: release.wrapperVersion,
  };
}

function referenceHeadings(
  page: ReferencePageData,
  namespacedChildrenCount: number,
) {
  if (page.page_kind === 'package' || page.page_kind === 'namespace') {
    return [{ depth: 2 as const, id: 'contents', label: 'Contents' }];
  }

  const declaration = exportedItemSchema.parse(page.declaration);
  return [
    { depth: 2 as const, id: 'signature', label: 'Signature' },
    ...declarationMemberGroups(declaration).map((group) => ({
      depth: 2 as const,
      id: group.id,
      label: group.title,
    })),
    ...(page.implementations.length > 0
      ? [{ depth: 2 as const, id: 'implementations', label: 'Implementations' }]
      : []),
    ...(page.cross_references.length > 0
      ? [{ depth: 2 as const, id: 'related', label: 'Related definitions' }]
      : []),
    ...(namespacedChildrenCount > 0
      ? [
          {
            depth: 2 as const,
            id: 'namespaced-definitions',
            label: 'Namespaced definitions',
          },
        ]
      : []),
  ];
}

function referenceSearchEntries(
  page: ReferencePageData,
): DocumentSearchEntry[] {
  const entries: DocumentSearchEntry[] = [
    {
      anchor: null,
      keywords: `${page.page_kind} ${page.summary ?? ''}`.trim(),
      label: page.qualified_name,
    },
  ];
  if ('member_anchors' in page) {
    entries.push(
      ...page.member_anchors.map((member) => ({
        anchor: member.anchor,
        keywords: member.member_kind,
        label: `${page.qualified_name}.${member.label}`,
      })),
    );
  }
  return entries;
}

function cliHeadings(command: CliCommandNodeInput) {
  return [
    { depth: 2 as const, id: 'usage', label: 'Usage' },
    ...(command.subcommands.length > 0
      ? [{ depth: 2 as const, id: 'subcommands', label: 'Subcommands' }]
      : []),
    ...(command.arguments.length > 0
      ? [{ depth: 2 as const, id: 'arguments', label: 'Arguments' }]
      : []),
    ...(command.flags.length > 0
      ? [{ depth: 2 as const, id: 'options', label: 'Options' }]
      : []),
  ];
}

function createRoute(
  path: string,
  snapshotInput: DocumentSnapshot,
  metadataInput: DocumentRouteMetadata,
): DocumentRouteInput {
  const snapshot = documentSnapshotSchema.parse(snapshotInput);
  const metadata = documentRouteMetadataSchema.parse(metadataInput);
  return {
    contentHash: hashDocumentSnapshot(snapshot),
    metadata,
    path,
    searchableText: snapshotSearchableText(snapshot),
    snapshot,
  };
}

function packageRoutes(
  release: CompleteReleasePublicationInput,
): DocumentRouteInput[] {
  const routeVersion = canonicalVersionToRouteVersion(release.version);
  const pages = release.packages
    .flatMap((packageInput) => packageInput.pages)
    .sort((left, right) => left.routePath.localeCompare(right.routePath));
  const packagePages = pages.filter((page) => page.pageKind === 'package');
  const packageIndexSnapshot: DocumentSnapshot = {
    blocks: [
      {
        packages: packagePages.map((page) => ({
          display_name: page.pageData.display_name,
          page_kind: page.pageKind,
          qualified_name: page.qualifiedName,
          route_path: page.routePath,
        })),
        type: 'packageIndex',
      },
    ],
    description: 'Versioned reference generated from the exact BAML toolchain.',
    headings: [
      { depth: 2, id: 'packages', label: 'Packages' },
      { depth: 2, id: 'release', label: 'Release provenance' },
    ],
    schemaVersion: DOCUMENT_SCHEMA_VERSION,
    title: 'Standard packages',
  };
  const routes = [
    createRoute('baml/packages', packageIndexSnapshot, {
      ...baseMetadata(release, {
        description: `Immutable package reference generated from BAML ${release.version}.`,
        publicPath: `/baml/packages/${routeVersion}`,
        searchEntries: [
          {
            anchor: null,
            keywords: 'standard library package index',
            label: `Standard packages ${routeVersion}`,
          },
        ],
        title: `Standard packages ${routeVersion}`,
      }),
      kind: 'packageIndex',
      surface: 'packages',
    }),
  ];

  for (const page of pages) {
    const namespacedChildren =
      page.pageKind === 'package' || page.pageKind === 'namespace'
        ? []
        : pages
            .filter((candidate) => {
              const prefix = `${page.routePath}/`;
              return (
                candidate.routePath.startsWith(prefix) &&
                !candidate.routePath.slice(prefix.length).includes('/')
              );
            })
            .map((child) => ({
              display_name: child.pageData.display_name,
              page_kind: child.pageKind,
              qualified_name: child.qualifiedName,
              route_path: child.routePath,
            }));
    const description =
      page.pageData.summary ??
      `${page.pageKind} in the ${page.pageData.package_name} package.`;
    const snapshot: DocumentSnapshot = {
      blocks: [
        {
          namespacedChildren,
          page: page.pageData,
          type: 'bamlReference',
        },
      ],
      description,
      headings: referenceHeadings(page.pageData, namespacedChildren.length),
      schemaVersion: DOCUMENT_SCHEMA_VERSION,
      title: page.qualifiedName,
    };
    routes.push(
      createRoute(`baml/packages/${page.routePath}`, snapshot, {
        ...baseMetadata(release, {
          description,
          publicPath: `/baml/packages/${routeVersion}/${page.routePath}`,
          searchEntries: referenceSearchEntries(page.pageData),
          title: `${page.qualifiedName} — ${routeVersion}`,
        }),
        kind: 'packageReference',
        pageKind: page.pageKind,
        qualifiedName: page.qualifiedName,
        surface: 'packages',
      }),
    );
  }
  return routes;
}

function cliRoutes(
  release: CompleteReleasePublicationInput,
): DocumentRouteInput[] {
  const routeVersion = canonicalVersionToRouteVersion(release.version);
  const root = release.cli.payload.root;
  const description =
    root.description ?? `Exact command reference for BAML ${release.version}.`;
  const routes = [
    createRoute(
      'cli',
      {
        blocks: [{ root, type: 'cliOverview' }],
        description: root.description,
        headings: [
          { depth: 2, id: 'usage', label: 'Usage' },
          { depth: 2, id: 'commands', label: 'Commands' },
          { depth: 2, id: 'release', label: 'Release provenance' },
        ],
        schemaVersion: DOCUMENT_SCHEMA_VERSION,
        title: 'BAML CLI',
      },
      {
        ...baseMetadata(release, {
          description,
          publicPath: `/cli/${routeVersion}`,
          searchEntries: [
            {
              anchor: null,
              keywords: 'command line interface overview',
              label: `BAML CLI ${routeVersion}`,
            },
          ],
          title: `BAML CLI ${routeVersion}`,
        }),
        kind: 'cliOverview',
        surface: 'cli',
      },
    ),
    createRoute(
      'cli/commands',
      {
        blocks: [{ commands: root.subcommands, type: 'cliCommandIndex' }],
        description:
          'Every public command captured from the exact BAML executable.',
        headings: [{ depth: 2, id: 'commands', label: 'Commands' }],
        schemaVersion: DOCUMENT_SCHEMA_VERSION,
        title: 'Command index',
      },
      {
        ...baseMetadata(release, {
          description: `Every public command captured from the exact BAML ${release.version} executable.`,
          publicPath: `/cli/${routeVersion}/commands`,
          searchEntries: [
            {
              anchor: null,
              keywords: 'command index',
              label: `Command index ${routeVersion}`,
            },
          ],
          title: `Command index — ${routeVersion}`,
        }),
        kind: 'cliCommandIndex',
        surface: 'cli',
      },
    ),
  ];

  for (const command of flattenCliCommands(root)) {
    const commandLabel = `baml ${command.command_path.join(' ')}`;
    const commandPath = command.command_path.join('/');
    const commandDescription =
      command.description ?? `Reference for ${commandLabel}.`;
    routes.push(
      createRoute(
        `cli/commands/${commandPath}`,
        {
          blocks: [{ command, type: 'cliCommand' }],
          description: commandDescription,
          headings: cliHeadings(command),
          schemaVersion: DOCUMENT_SCHEMA_VERSION,
          title: commandLabel,
        },
        {
          ...baseMetadata(release, {
            description: commandDescription,
            publicPath: `/cli/${routeVersion}/commands/${commandPath}`,
            searchEntries: [
              {
                anchor: null,
                keywords: command.description ?? '',
                label: commandLabel,
              },
            ],
            title: `${commandLabel} — ${routeVersion}`,
          }),
          commandPath: command.command_path,
          kind: 'cliCommand',
          surface: 'cli',
        },
      ),
    );
  }
  return routes;
}

function validateLogicalLinks(routes: readonly DocumentRouteInput[]): void {
  const routePaths = new Set(routes.map((route) => route.path));
  for (const route of routes) {
    for (const block of route.snapshot.blocks) {
      if (block.type !== 'bamlReference') continue;
      const linkedPaths = [
        ...(block.page.page_kind === 'package' ||
        block.page.page_kind === 'namespace'
          ? block.page.children.map((child) => child.route_path)
          : []),
        ...('cross_references' in block.page
          ? block.page.cross_references.map((reference) => reference.route_path)
          : []),
        ...block.namespacedChildren.map((child) => child.route_path),
      ];
      for (const linkedPath of linkedPaths) {
        if (!routePaths.has(`baml/packages/${linkedPath}`)) {
          throw new Error(
            `Document ${route.path} links to missing reference ${linkedPath}.`,
          );
        }
      }
    }
  }
}

export function projectDocumentRelease(
  release: CompleteReleasePublicationInput,
): DocumentReleaseBundle {
  const routes = [...packageRoutes(release), ...cliRoutes(release)].sort(
    (left, right) => left.path.localeCompare(right.path),
  );
  const paths = new Set<string>();
  for (const route of routes) {
    if (paths.has(route.path)) {
      throw new Error(`Duplicate document route: ${route.path}.`);
    }
    paths.add(route.path);
  }
  validateLogicalLinks(routes);

  const snapshots = new Map<string, DocumentSnapshot>();
  for (const route of routes) {
    const existing = snapshots.get(route.contentHash);
    if (
      existing &&
      canonicalJson(jsonValueSchema.parse(existing)) !==
        canonicalJson(jsonValueSchema.parse(route.snapshot))
    ) {
      throw new Error(`SHA-256 collision for snapshot ${route.contentHash}.`);
    }
    snapshots.set(route.contentHash, route.snapshot);
  }
  const manifest = {
    contentSchemaVersion: DOCUMENT_SCHEMA_VERSION,
    routes: routes.map((route) => ({
      contentHash: route.contentHash,
      metadata: route.metadata,
      path: route.path,
    })),
    sourceRevision: release.sourceCommit,
    version: release.version,
  };

  return {
    contentSchemaVersion: DOCUMENT_SCHEMA_VERSION,
    generatedAt: release.generatedAt,
    generatorVersion: release.generatorVersion,
    manifestHash: sha256(canonicalJson(jsonValueSchema.parse(manifest))),
    releasedAt: release.releasedAt,
    routes,
    snapshots,
    sourceRevision: release.sourceCommit,
    version: release.version,
    wrapperVersion: release.wrapperVersion,
  };
}
