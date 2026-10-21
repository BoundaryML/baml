export const PAGE_SCHEMA_VERSION = 1 as const;
export const CLI_ARTIFACT_SCHEMA_VERSION = 1 as const;
export const DOCUMENT_SCHEMA_VERSION = 1 as const;
export const GENERATED_CONTENT_DATABASE_ENVIRONMENT_VARIABLE =
  'GENERATED_CONTENT_DATABASE_URL' as const;
export const GENERATED_CONTENT_PUBLISHER_DATABASE_ENVIRONMENT_VARIABLE =
  'GENERATED_CONTENT_PUBLISHER_DATABASE_URL' as const;
export const GENERATED_CONTENT_MIGRATION_DATABASE_ENVIRONMENT_VARIABLE =
  'GENERATED_CONTENT_MIGRATION_DATABASE_URL' as const;

export const CHANNELS = ['stable', 'canary', 'nightly'] as const;
export const PAGE_KINDS = [
  'package',
  'namespace',
  'class',
  'enum',
  'interface',
  'type_alias',
  'function',
] as const;

export const DECLARATION_PAGE_KINDS = [
  'class',
  'enum',
  'interface',
  'type_alias',
  'function',
] as const;
