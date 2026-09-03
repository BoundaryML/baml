import { z } from 'zod';

import { invokeBaml } from '@/lib/generated-content/baml-binary';
import { PAGE_SCHEMA_VERSION } from '@/lib/generated-content/constants';
import {
  type JsonValue,
  jsonValueSchema,
  sha256,
} from '@/lib/generated-content/json';
import {
  createMemberAnchors,
  qualifiedNameToRoutePath,
  qualifyExportedName,
} from '@/lib/generated-content/routes';
import {
  declarationPageKindSchema,
  packageDescribeExportSchema,
  type ReferencePageData,
  referencePageDataSchema,
} from '@/lib/generated-content/schemas';

const exportedMemberSchema = z
  .object({
    id: z.string().min(1),
    name: z.string().min(1),
  })
  .passthrough();

const exportedItemSchema = z
  .object({
    assoc_types: z.array(exportedMemberSchema).optional(),
    default_methods: z.array(exportedMemberSchema).optional(),
    docstring: z.string().optional(),
    fields: z.array(exportedMemberSchema).optional(),
    id: z.string().min(1),
    impls: z.array(z.string().min(1)).optional(),
    kind: declarationPageKindSchema,
    methods: z.array(exportedMemberSchema).optional(),
    name: z.string().min(1),
    namespace: z.array(z.string().min(1)).optional(),
    required_methods: z.array(exportedMemberSchema).optional(),
    variants: z.array(exportedMemberSchema).optional(),
  })
  .passthrough();

const exportedImplementationSchema = z
  .object({
    docstring: z.string().optional(),
    id: z.string().min(1),
    methods: z
      .array(
        z
          .object({
            id: z.string().min(1),
            name: z.string().min(1),
          })
          .passthrough(),
      )
      .optional(),
  })
  .passthrough();

type ExportedItem = z.output<typeof exportedItemSchema>;
type ExportedImplementation = z.output<typeof exportedImplementationSchema>;

export interface ReferencePageProjection {
  pageData: ReferencePageData;
  pageKind: ReferencePageData['page_kind'];
  qualifiedName: string;
  routePath: string;
}

export interface PackagePublicationInput {
  describeFormatVersion: number;
  describeOutputJson: string;
  describeSha256: string;
  packageName: string;
  pages: ReferencePageProjection[];
}

interface DeclarationRecord {
  anchors: ProjectedMemberAnchor[];
  implementations: ExportedImplementation[];
  item: ExportedItem;
  namespace: string[];
  qualifiedName: string;
  routePath: string;
}

type ProjectedMemberAnchor = ReturnType<typeof createMemberAnchors>[number] & {
  memberKind: string;
};

function firstParagraph(docstring: string | undefined): string | null {
  if (!docstring) {
    return null;
  }
  return docstring
    .split(/\n\s*\n/, 1)[0]
    .replaceAll('\n', ' ')
    .trim();
}

function collectAnchors(
  item: ExportedItem,
  implementations: ExportedImplementation[],
): ProjectedMemberAnchor[] {
  const inputs: { exportedId: string; label: string; memberKind: string }[] =
    [];
  const memberGroups = [
    [item.fields, 'field'],
    [item.methods, 'method'],
    [item.variants, 'variant'],
    [item.assoc_types, 'associated_type'],
    [item.required_methods, 'required_method'],
    [item.default_methods, 'default_method'],
  ] as const;

  for (const [members, memberKind] of memberGroups) {
    for (const member of members ?? []) {
      inputs.push({
        exportedId: member.id,
        label: member.name,
        memberKind,
      });
    }
  }

  for (const implementation of implementations) {
    inputs.push({
      exportedId: implementation.id,
      label: 'impl',
      memberKind: 'implementation',
    });
    for (const method of implementation.methods ?? []) {
      inputs.push({
        exportedId: method.id,
        label: method.name,
        memberKind: 'implementation_method',
      });
    }
  }

  const kindsById = new Map(
    inputs.map((input) => [input.exportedId, input.memberKind]),
  );
  return createMemberAnchors(inputs).map((anchor) => ({
    ...anchor,
    memberKind: kindsById.get(anchor.exportedId) ?? 'member',
  }));
}

function collectKnownCrossReferences(
  value: JsonValue,
  targets: Map<
    string,
    { anchor: string | null; qualifiedName: string; routePath: string }
  >,
  output: Set<string>,
): void {
  const stringValue = z.string().safeParse(value);
  if (stringValue.success) {
    if (targets.has(stringValue.data)) {
      output.add(stringValue.data);
    }
    return;
  }

  if (Array.isArray(value)) {
    for (const child of value) {
      collectKnownCrossReferences(child, targets, output);
    }
    return;
  }

  const objectValue = z.record(z.string(), jsonValueSchema).safeParse(value);
  if (!objectValue.success) return;
  for (const child of Object.values(objectValue.data)) {
    collectKnownCrossReferences(child, targets, output);
  }
}

function buildReferencePages(
  packageName: string,
  formatVersion: number,
  items: ExportedItem[],
  implementations: ExportedImplementation[],
): ReferencePageProjection[] {
  const implementationById = new Map(
    implementations.map((implementation) => [
      implementation.id,
      implementation,
    ]),
  );
  const declarations: DeclarationRecord[] = items.map((item) => {
    const namespace = item.namespace ?? [];
    const qualifiedName = qualifyExportedName(
      packageName,
      namespace,
      item.name,
    );
    const implementationIds = item.impls ?? [];
    const resolvedImplementations = implementationIds.flatMap((id) => {
      const implementation = implementationById.get(id);
      return implementation ? [implementation] : [];
    });
    return {
      anchors: collectAnchors(item, resolvedImplementations),
      implementations: resolvedImplementations,
      item,
      namespace,
      qualifiedName,
      routePath: qualifiedNameToRoutePath(qualifiedName),
    };
  });

  const routePaths = new Set<string>();
  const qualifiedNames = new Set<string>();
  for (const declaration of declarations) {
    if (routePaths.has(declaration.routePath)) {
      throw new Error(`Duplicate package route: ${declaration.routePath}.`);
    }
    if (qualifiedNames.has(declaration.qualifiedName)) {
      throw new Error(
        `Duplicate package qualified name: ${declaration.qualifiedName}.`,
      );
    }
    routePaths.add(declaration.routePath);
    qualifiedNames.add(declaration.qualifiedName);
  }

  const namespaceKeys = new Set<string>();
  for (const declaration of declarations) {
    for (let length = 1; length <= declaration.namespace.length; length += 1) {
      namespaceKeys.add(declaration.namespace.slice(0, length).join('.'));
    }
  }

  const targetByExportedId = new Map<
    string,
    { anchor: string | null; qualifiedName: string; routePath: string }
  >();
  const implementationAnchorOwnerCounts = new Map<string, number>();
  for (const declaration of declarations) {
    for (const anchor of declaration.anchors) {
      if (anchor.memberKind.startsWith('implementation')) {
        implementationAnchorOwnerCounts.set(
          anchor.exportedId,
          (implementationAnchorOwnerCounts.get(anchor.exportedId) ?? 0) + 1,
        );
      }
    }
  }
  for (const declaration of declarations) {
    targetByExportedId.set(declaration.item.id, {
      anchor: null,
      qualifiedName: declaration.qualifiedName,
      routePath: declaration.routePath,
    });
    for (const anchor of declaration.anchors) {
      const isUnambiguousOwner =
        !anchor.memberKind.startsWith('implementation') ||
        implementationAnchorOwnerCounts.get(anchor.exportedId) === 1;
      if (isUnambiguousOwner && !targetByExportedId.has(anchor.exportedId)) {
        targetByExportedId.set(anchor.exportedId, {
          anchor: anchor.anchor,
          qualifiedName: declaration.qualifiedName,
          routePath: declaration.routePath,
        });
      }
    }
  }

  const childForDeclaration = (declaration: DeclarationRecord) => ({
    display_name: declaration.item.name,
    page_kind: declaration.item.kind,
    qualified_name: declaration.qualifiedName,
    route_path: declaration.routePath,
  });
  const childForNamespace = (namespacePath: string[]) => {
    const qualifiedName = [packageName, ...namespacePath].join('.');
    return {
      display_name: namespacePath.at(-1) ?? packageName,
      page_kind: 'namespace' as const,
      qualified_name: qualifiedName,
      route_path: qualifiedNameToRoutePath(qualifiedName),
    };
  };

  const packageChildren = [
    ...[...namespaceKeys]
      .filter((key) => !key.includes('.'))
      .map((key) => childForNamespace([key])),
    ...declarations
      .filter((declaration) => declaration.namespace.length === 0)
      .map(childForDeclaration),
  ].sort((left, right) =>
    left.qualified_name.localeCompare(right.qualified_name),
  );

  const pages: ReferencePageProjection[] = [
    {
      pageData: referencePageDataSchema.parse({
        children: packageChildren,
        describe_format_version: formatVersion,
        display_name: packageName,
        package_name: packageName,
        page_kind: 'package',
        qualified_name: packageName,
        schema_version: PAGE_SCHEMA_VERSION,
        summary: null,
      }),
      pageKind: 'package',
      qualifiedName: packageName,
      routePath: qualifiedNameToRoutePath(packageName),
    },
  ];

  for (const key of [...namespaceKeys].sort()) {
    const namespacePath = key.split('.');
    const qualifiedName = [packageName, ...namespacePath].join('.');
    const childNamespaces = [...namespaceKeys]
      .map((candidate) => candidate.split('.'))
      .filter(
        (candidate) =>
          candidate.length === namespacePath.length + 1 &&
          candidate.slice(0, -1).join('.') === key,
      )
      .map(childForNamespace);
    const childDeclarations = declarations
      .filter((declaration) => declaration.namespace.join('.') === key)
      .map(childForDeclaration);
    const pageData = referencePageDataSchema.parse({
      children: [...childNamespaces, ...childDeclarations].sort((left, right) =>
        left.qualified_name.localeCompare(right.qualified_name),
      ),
      display_name: namespacePath.at(-1),
      namespace_path: namespacePath,
      package_name: packageName,
      page_kind: 'namespace',
      qualified_name: qualifiedName,
      schema_version: PAGE_SCHEMA_VERSION,
      summary: null,
    });
    pages.push({
      pageData,
      pageKind: pageData.page_kind,
      qualifiedName,
      routePath: qualifiedNameToRoutePath(qualifiedName),
    });
  }

  for (const declaration of declarations) {
    const referencedIds = new Set<string>();
    collectKnownCrossReferences(
      jsonValueSchema.parse(declaration.item),
      targetByExportedId,
      referencedIds,
    );
    for (const implementation of declaration.implementations) {
      collectKnownCrossReferences(
        jsonValueSchema.parse(implementation),
        targetByExportedId,
        referencedIds,
      );
    }
    referencedIds.delete(declaration.item.id);
    const pageData = referencePageDataSchema.parse({
      cross_references: [...referencedIds]
        .map((exportedId) => {
          const target = targetByExportedId.get(exportedId);
          if (!target) {
            throw new Error(
              `Missing known cross-reference target: ${exportedId}.`,
            );
          }
          return {
            anchor: target.anchor,
            exported_id: exportedId,
            qualified_name: target.qualifiedName,
            route_path: target.routePath,
          };
        })
        .filter(
          (reference) => reference.qualified_name !== declaration.qualifiedName,
        )
        .sort((left, right) =>
          left.exported_id.localeCompare(right.exported_id),
        ),
      declaration: declaration.item,
      display_name: declaration.item.name,
      exported_id: declaration.item.id,
      implementations: declaration.implementations,
      member_anchors: declaration.anchors.map((anchor) => ({
        anchor: anchor.anchor,
        exported_id: anchor.exportedId,
        label: anchor.label,
        member_kind: anchor.memberKind,
      })),
      namespace_path: declaration.namespace,
      package_name: packageName,
      page_kind: declaration.item.kind,
      qualified_name: declaration.qualifiedName,
      schema_version: PAGE_SCHEMA_VERSION,
      summary: firstParagraph(declaration.item.docstring),
    });
    pages.push({
      pageData,
      pageKind: pageData.page_kind,
      qualifiedName: declaration.qualifiedName,
      routePath: declaration.routePath,
    });
  }

  const allRoutes = new Set<string>();
  for (const page of pages) {
    if (allRoutes.has(page.routePath)) {
      throw new Error(`Projected package route collision: ${page.routePath}.`);
    }
    allRoutes.add(page.routePath);
  }
  return pages.sort((left, right) =>
    left.routePath.localeCompare(right.routePath),
  );
}

export async function generatePackagePublicationInput(
  bamlBinary: string,
  packageName: string,
): Promise<PackagePublicationInput> {
  const invocation = await invokeBaml(bamlBinary, [
    'describe',
    packageName,
    '--export',
  ]);
  const rawPayload = packageDescribeExportSchema.parse(
    JSON.parse(invocation.stdout),
  );
  if (rawPayload.package !== packageName) {
    throw new Error(
      `Selected binary exported ${rawPayload.package} when ${packageName} was requested.`,
    );
  }
  const items = exportedItemSchema.array().parse(rawPayload.items);
  const implementations = exportedImplementationSchema
    .array()
    .parse(rawPayload.impls);
  return {
    describeFormatVersion: rawPayload.format_version,
    describeOutputJson: invocation.stdout,
    describeSha256: sha256(invocation.stdout),
    packageName,
    pages: buildReferencePages(
      packageName,
      rawPayload.format_version,
      items,
      implementations,
    ),
  };
}
