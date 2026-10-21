CREATE SCHEMA IF NOT EXISTS developer_docs;

CREATE TABLE developer_docs.releases (
  version             TEXT PRIMARY KEY,
  source_commit       TEXT NOT NULL,
  released_at         TIMESTAMPTZ NOT NULL,
  generated_at        TIMESTAMPTZ NOT NULL,
  generator_version   TEXT NOT NULL,
  created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),

  CHECK (version <> ''),
  CHECK (source_commit ~ '^[0-9a-f]{40}$')
);

CREATE TABLE developer_docs.channel_pointers (
  channel          TEXT PRIMARY KEY,
  release_version  TEXT NOT NULL
    REFERENCES developer_docs.releases(version)
    ON DELETE RESTRICT,
  updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),

  CHECK (channel IN ('stable', 'canary', 'nightly'))
);

CREATE TABLE developer_docs.package_exports (
  id                       BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  release_version          TEXT NOT NULL
    REFERENCES developer_docs.releases(version)
    ON DELETE RESTRICT,
  package_name             TEXT NOT NULL,
  describe_format_version  INTEGER NOT NULL,
  describe_output_json     TEXT NOT NULL,
  describe_sha256          TEXT NOT NULL,
  generated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (release_version, package_name),
  CHECK (package_name <> ''),
  CHECK (describe_format_version > 0),
  CHECK (describe_sha256 ~ '^[0-9a-f]{64}$')
);

CREATE TABLE developer_docs.reference_pages (
  package_export_id     BIGINT NOT NULL
    REFERENCES developer_docs.package_exports(id)
    ON DELETE RESTRICT,
  page_schema_version   INTEGER NOT NULL,
  qualified_name        TEXT NOT NULL,
  page_kind             TEXT NOT NULL,
  route_path            TEXT NOT NULL,
  page_data             JSONB NOT NULL,
  generated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),

  PRIMARY KEY (
    package_export_id,
    page_schema_version,
    qualified_name
  ),

  UNIQUE (
    package_export_id,
    page_schema_version,
    route_path
  ),

  CHECK (page_schema_version > 0),
  CHECK (qualified_name <> ''),
  CHECK (route_path <> ''),
  CHECK (
    page_kind IN (
      'package',
      'namespace',
      'class',
      'enum',
      'interface',
      'type_alias',
      'function'
    )
  )
);

CREATE TABLE developer_docs.cli_artifacts (
  release_version          TEXT PRIMARY KEY
    REFERENCES developer_docs.releases(version)
    ON DELETE RESTRICT,
  wrapper_version          TEXT NOT NULL,
  artifact_schema_version  INTEGER NOT NULL,
  source_sha256            TEXT NOT NULL,
  payload_sha256           TEXT NOT NULL,
  payload_json             TEXT NOT NULL,
  generated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),

  CHECK (wrapper_version <> ''),
  CHECK (artifact_schema_version > 0),
  CHECK (source_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK (payload_sha256 ~ '^[0-9a-f]{64}$')
);
