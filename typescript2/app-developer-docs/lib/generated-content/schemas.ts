import { z } from 'zod';

import {
  CHANNELS,
  CLI_ARTIFACT_SCHEMA_VERSION,
  DECLARATION_PAGE_KINDS,
  PAGE_KINDS,
  PAGE_SCHEMA_VERSION,
} from '@/lib/generated-content/constants';
import { jsonValueSchema } from '@/lib/generated-content/json';
import { qualifiedNameToRoutePath } from '@/lib/generated-content/routes';

export { jsonValueSchema };

const nonEmptyStringSchema = z.string().min(1);
const sha256Schema = z.string().regex(/^[0-9a-f]{64}$/);
const sourceCommitSchema = z.string().regex(/^[0-9a-f]{40}$/);
const positiveIntegerSchema = z.number().int().positive();
const identifierSchema = z.string().regex(/^[A-Za-z_$][A-Za-z0-9_$]*$/);

const jsonObjectSchema = z.record(z.string(), jsonValueSchema);

const timestampSchema = z
  .union([z.date(), z.string().min(1)])
  .transform((value, context) => {
    const timestamp = value instanceof Date ? value : new Date(value);
    if (!Number.isFinite(timestamp.valueOf())) {
      context.addIssue({
        code: 'custom',
        message: 'Expected an ISO timestamp or Date.',
      });
      return z.NEVER;
    }
    return timestamp;
  });

export const channelSchema = z.enum(CHANNELS);
export const pageKindSchema = z.enum(PAGE_KINDS);
export const declarationPageKindSchema = z.enum(DECLARATION_PAGE_KINDS);

export const releaseRowSchema = z
  .object({
    created_at: timestampSchema,
    generated_at: timestampSchema,
    generator_version: nonEmptyStringSchema,
    released_at: timestampSchema,
    source_commit: sourceCommitSchema,
    version: nonEmptyStringSchema,
  })
  .strict();

export const channelPointerRowSchema = z
  .object({
    channel: channelSchema,
    release_version: nonEmptyStringSchema,
    updated_at: timestampSchema,
  })
  .strict();

export const packageExportRowSchema = z
  .object({
    describe_format_version: positiveIntegerSchema,
    describe_output_json: nonEmptyStringSchema,
    describe_sha256: sha256Schema,
    generated_at: timestampSchema,
    id: z.union([
      z.bigint(),
      z.number().int().positive(),
      z.string().regex(/^\d+$/),
    ]),
    package_name: identifierSchema,
    release_version: nonEmptyStringSchema,
  })
  .strict();

export const packageDescribeExportSchema = z
  .object({
    format_version: positiveIntegerSchema,
    impls: z.array(jsonObjectSchema),
    items: z.array(jsonObjectSchema),
    package: identifierSchema,
  })
  .strict();

export const referenceChildSchema = z
  .object({
    display_name: nonEmptyStringSchema,
    page_kind: pageKindSchema,
    qualified_name: nonEmptyStringSchema,
    route_path: nonEmptyStringSchema,
  })
  .strict();

const referencePageBaseSchema = z.object({
  display_name: nonEmptyStringSchema,
  package_name: identifierSchema,
  qualified_name: nonEmptyStringSchema,
  schema_version: z.literal(PAGE_SCHEMA_VERSION),
  summary: z.string().nullable(),
});

export const packagePageDataSchema = referencePageBaseSchema
  .extend({
    children: z.array(referenceChildSchema),
    describe_format_version: positiveIntegerSchema,
    page_kind: z.literal('package'),
  })
  .strict();

export const namespacePageDataSchema = referencePageBaseSchema
  .extend({
    children: z.array(referenceChildSchema),
    namespace_path: z.array(identifierSchema).min(1),
    page_kind: z.literal('namespace'),
  })
  .strict();

export const memberAnchorSchema = z
  .object({
    anchor: identifierSchema.or(
      z.string().regex(/^[A-Za-z_$][A-Za-z0-9_$]*-[0-9a-f]{8}$/),
    ),
    exported_id: nonEmptyStringSchema,
    label: identifierSchema,
    member_kind: nonEmptyStringSchema,
  })
  .strict();

export const crossReferenceSchema = z
  .object({
    anchor: nonEmptyStringSchema.nullable(),
    exported_id: nonEmptyStringSchema,
    qualified_name: nonEmptyStringSchema,
    route_path: nonEmptyStringSchema,
  })
  .strict();

export const declarationPageDataSchema = referencePageBaseSchema
  .extend({
    cross_references: z.array(crossReferenceSchema),
    declaration: jsonObjectSchema,
    exported_id: nonEmptyStringSchema,
    implementations: z.array(jsonObjectSchema),
    member_anchors: z.array(memberAnchorSchema),
    namespace_path: z.array(identifierSchema),
    page_kind: declarationPageKindSchema,
  })
  .strict();

export const referencePageDataSchema = z.discriminatedUnion('page_kind', [
  packagePageDataSchema,
  namespacePageDataSchema,
  declarationPageDataSchema,
]);

export const referencePageRowSchema = z
  .object({
    generated_at: timestampSchema,
    package_export_id: z.union([
      z.bigint(),
      z.number().int().positive(),
      z.string().regex(/^\d+$/),
    ]),
    page_data: referencePageDataSchema,
    page_kind: pageKindSchema,
    page_schema_version: z.literal(PAGE_SCHEMA_VERSION),
    qualified_name: nonEmptyStringSchema,
    route_path: nonEmptyStringSchema,
  })
  .strict()
  .superRefine((row, context) => {
    if (row.page_kind !== row.page_data.page_kind) {
      context.addIssue({
        code: 'custom',
        message: 'Row and payload page kinds do not match.',
      });
    }
    if (row.qualified_name !== row.page_data.qualified_name) {
      context.addIssue({
        code: 'custom',
        message: 'Row and payload qualified names do not match.',
      });
    }
    if (row.route_path !== qualifiedNameToRoutePath(row.qualified_name)) {
      context.addIssue({
        code: 'custom',
        message: 'Row route does not match its qualified name.',
      });
    }
  });

const cliArgumentSchema = z
  .object({
    allowed_values: z.array(z.string()),
    default_value: z.string().nullable(),
    description: z.string().nullable(),
    name: nonEmptyStringSchema,
    required: z.boolean(),
  })
  .strict();

const cliFlagSchema = z
  .object({
    allowed_values: z.array(z.string()),
    default_value: z.string().nullable(),
    description: z.string().nullable(),
    long: z.string().nullable(),
    short: z.string().nullable(),
    value_name: z.string().nullable(),
  })
  .strict()
  .refine((flag) => flag.long !== null || flag.short !== null, {
    message: 'A CLI flag requires a long or short spelling.',
  });

export interface CliCommandNodeInput {
  arguments: z.input<typeof cliArgumentSchema>[];
  command_path: string[];
  description: string | null;
  flags: z.input<typeof cliFlagSchema>[];
  name: string;
  subcommands: CliCommandNodeInput[];
  usage: string;
}

export const cliCommandNodeSchema: z.ZodType<CliCommandNodeInput> = z.lazy(() =>
  z
    .object({
      arguments: z.array(cliArgumentSchema),
      command_path: z.array(nonEmptyStringSchema),
      description: z.string().nullable(),
      flags: z.array(cliFlagSchema),
      name: nonEmptyStringSchema,
      subcommands: z.array(cliCommandNodeSchema),
      usage: nonEmptyStringSchema,
    })
    .strict(),
);

export const rawCliHelpSchema = z
  .object({
    command_path: z.array(nonEmptyStringSchema),
    invocation: z.array(nonEmptyStringSchema),
    sha256: sha256Schema,
    text: nonEmptyStringSchema,
  })
  .strict();

export const cliArtifactPayloadSchema = z
  .object({
    artifact_schema_version: z.literal(CLI_ARTIFACT_SCHEMA_VERSION),
    product_version: nonEmptyStringSchema,
    raw_help: z.array(rawCliHelpSchema).min(1),
    root: cliCommandNodeSchema,
    wrapper_version: nonEmptyStringSchema,
  })
  .strict();

export const cliArtifactRowSchema = z
  .object({
    artifact_schema_version: z.literal(CLI_ARTIFACT_SCHEMA_VERSION),
    generated_at: timestampSchema,
    payload_json: nonEmptyStringSchema,
    payload_sha256: sha256Schema,
    release_version: nonEmptyStringSchema,
    source_sha256: sha256Schema,
    wrapper_version: nonEmptyStringSchema,
  })
  .strict();

export type ReleaseRow = z.output<typeof releaseRowSchema>;
export type ChannelPointerRow = z.output<typeof channelPointerRowSchema>;
export type PackageExportRow = z.output<typeof packageExportRowSchema>;
export type ReferencePageData = z.output<typeof referencePageDataSchema>;
export type ReferencePageRow = z.output<typeof referencePageRowSchema>;
export type CliArtifactPayload = z.output<typeof cliArtifactPayloadSchema>;
export type CliArtifactRow = z.output<typeof cliArtifactRowSchema>;
