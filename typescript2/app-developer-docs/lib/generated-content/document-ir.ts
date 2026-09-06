import { z } from 'zod';

import { DOCUMENT_SCHEMA_VERSION } from '@/lib/generated-content/constants';
import {
  canonicalJson,
  jsonValueSchema,
  sha256,
} from '@/lib/generated-content/json';
import {
  cliCommandNodeSchema,
  referenceChildSchema,
  referencePageDataSchema,
} from '@/lib/generated-content/schemas';

const nonEmptyStringSchema = z.string().min(1);
const sha256Schema = z.string().regex(/^[0-9a-f]{64}$/);
const sourceRevisionSchema = z.string().regex(/^[0-9a-f]{40}$/);
const routeVersionSchema = z.string().regex(/^v[^/]+$/);
const routePathSchema = z
  .string()
  .min(1)
  .refine((value) => !value.startsWith('/') && !value.endsWith('/'), {
    message: 'Stored document paths are relative and omit trailing slashes.',
  });

export const documentHeadingSchema = z
  .object({
    depth: z.number().int().min(2).max(4),
    id: nonEmptyStringSchema,
    label: nonEmptyStringSchema,
  })
  .strict();

const packageIndexBlockSchema = z
  .object({
    packages: z.array(referenceChildSchema),
    type: z.literal('packageIndex'),
  })
  .strict();

const bamlReferenceBlockSchema = z
  .object({
    namespacedChildren: z.array(referenceChildSchema),
    page: referencePageDataSchema,
    type: z.literal('bamlReference'),
  })
  .strict();

const cliOverviewBlockSchema = z
  .object({
    root: cliCommandNodeSchema,
    type: z.literal('cliOverview'),
  })
  .strict();

const cliCommandIndexBlockSchema = z
  .object({
    commands: z.array(cliCommandNodeSchema),
    type: z.literal('cliCommandIndex'),
  })
  .strict();

const cliCommandBlockSchema = z
  .object({
    command: cliCommandNodeSchema,
    type: z.literal('cliCommand'),
  })
  .strict();

export const documentBlockSchema = z.discriminatedUnion('type', [
  packageIndexBlockSchema,
  bamlReferenceBlockSchema,
  cliOverviewBlockSchema,
  cliCommandIndexBlockSchema,
  cliCommandBlockSchema,
]);

export const documentSnapshotSchema = z
  .object({
    blocks: z.array(documentBlockSchema).min(1),
    description: z.string().nullable(),
    headings: z.array(documentHeadingSchema),
    schemaVersion: z.literal(DOCUMENT_SCHEMA_VERSION),
    title: nonEmptyStringSchema,
  })
  .strict();

export const documentSearchEntrySchema = z
  .object({
    anchor: z.string().min(1).nullable(),
    keywords: z.string(),
    label: nonEmptyStringSchema,
  })
  .strict();

const routeMetadataBaseSchema = z.object({
  canonicalVersion: nonEmptyStringSchema,
  description: z.string(),
  generatorVersion: sourceRevisionSchema,
  publicPath: z.string().startsWith('/'),
  releasedAt: z.string().datetime({ offset: true }),
  routeVersion: routeVersionSchema,
  searchEntries: z.array(documentSearchEntrySchema).min(1),
  sourceRevision: sourceRevisionSchema,
  title: nonEmptyStringSchema,
  wrapperVersion: nonEmptyStringSchema,
});

const packageIndexRouteMetadataSchema = routeMetadataBaseSchema
  .extend({
    kind: z.literal('packageIndex'),
    surface: z.literal('packages'),
  })
  .strict();

const packageReferenceRouteMetadataSchema = routeMetadataBaseSchema
  .extend({
    kind: z.literal('packageReference'),
    pageKind: nonEmptyStringSchema,
    qualifiedName: nonEmptyStringSchema,
    surface: z.literal('packages'),
  })
  .strict();

const cliOverviewRouteMetadataSchema = routeMetadataBaseSchema
  .extend({
    kind: z.literal('cliOverview'),
    surface: z.literal('cli'),
  })
  .strict();

const cliCommandIndexRouteMetadataSchema = routeMetadataBaseSchema
  .extend({
    kind: z.literal('cliCommandIndex'),
    surface: z.literal('cli'),
  })
  .strict();

const cliCommandRouteMetadataSchema = routeMetadataBaseSchema
  .extend({
    commandPath: z.array(nonEmptyStringSchema).min(1),
    kind: z.literal('cliCommand'),
    surface: z.literal('cli'),
  })
  .strict();

export const documentRouteMetadataSchema = z.discriminatedUnion('kind', [
  packageIndexRouteMetadataSchema,
  packageReferenceRouteMetadataSchema,
  cliOverviewRouteMetadataSchema,
  cliCommandIndexRouteMetadataSchema,
  cliCommandRouteMetadataSchema,
]);

const timestampSchema = z
  .union([z.date(), z.string().min(1)])
  .transform((value, context) => {
    const timestamp = value instanceof Date ? value : new Date(value);
    if (!Number.isFinite(timestamp.valueOf())) {
      context.addIssue({ code: 'custom', message: 'Expected a timestamp.' });
      return z.NEVER;
    }
    return timestamp;
  });

export const documentReleaseRowSchema = z
  .object({
    content_schema_version: z.literal(DOCUMENT_SCHEMA_VERSION),
    generator_version: sourceRevisionSchema,
    manifest_hash: sha256Schema,
    published_at: timestampSchema,
    released_at: timestampSchema,
    route_count: z.number().int().positive(),
    source_revision: sourceRevisionSchema,
    unique_snapshot_count: z.number().int().positive(),
    version: nonEmptyStringSchema,
    wrapper_version: nonEmptyStringSchema,
  })
  .strict()
  .refine((row) => row.unique_snapshot_count <= row.route_count, {
    message: 'Unique snapshot count cannot exceed the route count.',
  });

export const documentAliasRowSchema = z
  .object({
    alias: z.enum(['stable', 'canary', 'nightly']),
    updated_at: timestampSchema,
    version: nonEmptyStringSchema,
  })
  .strict();

export const storedDocumentRouteSchema = z
  .object({
    content: documentSnapshotSchema,
    content_hash: sha256Schema,
    path: routePathSchema,
    route_metadata: documentRouteMetadataSchema,
    schema_version: z.literal(DOCUMENT_SCHEMA_VERSION),
    searchable_text: z.string(),
    version: nonEmptyStringSchema,
  })
  .strict();

export const storedDocumentRouteIndexSchema = z
  .object({
    content_hash: sha256Schema,
    path: routePathSchema,
    route_metadata: documentRouteMetadataSchema,
    version: nonEmptyStringSchema,
  })
  .strict();

export interface DocumentRouteInput {
  contentHash: string;
  metadata: DocumentRouteMetadata;
  path: string;
  searchableText: string;
  snapshot: DocumentSnapshot;
}

export interface DocumentReleaseBundle {
  contentSchemaVersion: typeof DOCUMENT_SCHEMA_VERSION;
  generatedAt: string;
  generatorVersion: string;
  manifestHash: string;
  releasedAt: string;
  routes: DocumentRouteInput[];
  snapshots: Map<string, DocumentSnapshot>;
  sourceRevision: string;
  version: string;
  wrapperVersion: string;
}

export type DocumentBlock = z.output<typeof documentBlockSchema>;
export type DocumentSnapshot = z.output<typeof documentSnapshotSchema>;
export type DocumentSearchEntry = z.output<typeof documentSearchEntrySchema>;
export type DocumentRouteMetadata = z.output<
  typeof documentRouteMetadataSchema
>;
export type StoredDocumentRoute = z.output<typeof storedDocumentRouteSchema>;
export type StoredDocumentRouteIndex = z.output<
  typeof storedDocumentRouteIndexSchema
>;
export type DocumentReleaseRow = z.output<typeof documentReleaseRowSchema>;
export type DocumentAliasRow = z.output<typeof documentAliasRowSchema>;

export function canonicalDocumentSnapshot(snapshot: DocumentSnapshot): string {
  return canonicalJson(
    jsonValueSchema.parse(documentSnapshotSchema.parse(snapshot)),
  );
}

export function hashDocumentSnapshot(snapshot: DocumentSnapshot): string {
  return sha256(
    `${DOCUMENT_SCHEMA_VERSION}\n${canonicalDocumentSnapshot(snapshot)}`,
  );
}

export function verifyDocumentSnapshotHash(
  snapshot: DocumentSnapshot,
  expectedHash: string,
): void {
  const actualHash = hashDocumentSnapshot(snapshot);
  if (actualHash !== expectedHash) {
    throw new Error(
      `Document snapshot hash mismatch: expected ${expectedHash}, received ${actualHash}.`,
    );
  }
}
