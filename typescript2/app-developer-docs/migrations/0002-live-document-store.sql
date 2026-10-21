CREATE SCHEMA IF NOT EXISTS developer_docs;

CREATE TABLE IF NOT EXISTS developer_docs.doc_snapshots (
  content_hash      TEXT PRIMARY KEY,
  schema_version    INTEGER NOT NULL,
  content           JSONB NOT NULL,
  searchable_text   TEXT NOT NULL,
  created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),

  CHECK (content_hash ~ '^[0-9a-f]{64}$'),
  CHECK (schema_version > 0)
);

CREATE TABLE IF NOT EXISTS developer_docs.doc_releases (
  version                 TEXT PRIMARY KEY,
  source_revision         TEXT NOT NULL,
  released_at             TIMESTAMPTZ NOT NULL,
  generator_version       TEXT NOT NULL,
  wrapper_version         TEXT NOT NULL,
  content_schema_version  INTEGER NOT NULL,
  manifest_hash           TEXT NOT NULL,
  route_count             INTEGER NOT NULL,
  unique_snapshot_count   INTEGER NOT NULL,
  published_at            TIMESTAMPTZ NOT NULL DEFAULT now(),

  CHECK (version <> ''),
  CHECK (source_revision ~ '^[0-9a-f]{40}$'),
  CHECK (generator_version ~ '^[0-9a-f]{40}$'),
  CHECK (wrapper_version <> ''),
  CHECK (content_schema_version > 0),
  CHECK (manifest_hash ~ '^[0-9a-f]{64}$'),
  CHECK (route_count > 0),
  CHECK (unique_snapshot_count > 0),
  CHECK (unique_snapshot_count <= route_count)
);

CREATE TABLE IF NOT EXISTS developer_docs.doc_routes (
  version         TEXT NOT NULL
    REFERENCES developer_docs.doc_releases(version)
    ON DELETE RESTRICT,
  path            TEXT NOT NULL,
  content_hash    TEXT NOT NULL
    REFERENCES developer_docs.doc_snapshots(content_hash)
    ON DELETE RESTRICT,
  route_metadata  JSONB NOT NULL,

  PRIMARY KEY (version, path),
  CHECK (path <> ''),
  CHECK (path !~ '^/'),
  CHECK (path !~ '/$')
);

CREATE INDEX IF NOT EXISTS doc_routes_path_idx
  ON developer_docs.doc_routes(path, version);

CREATE TABLE IF NOT EXISTS developer_docs.doc_aliases (
  alias       TEXT PRIMARY KEY,
  version     TEXT NOT NULL
    REFERENCES developer_docs.doc_releases(version)
    ON DELETE RESTRICT,
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

  CHECK (alias IN ('stable', 'canary', 'nightly'))
);

CREATE OR REPLACE FUNCTION developer_docs.reject_immutable_document_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
  RAISE EXCEPTION '% rows are immutable after publication', TG_TABLE_NAME;
END;
$$;

CREATE OR REPLACE FUNCTION developer_docs.enforce_document_route_capacity()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
  release_version TEXT;
  expected_routes INTEGER;
  actual_routes INTEGER;
BEGIN
  FOR release_version IN SELECT DISTINCT version FROM inserted_routes LOOP
    SELECT route_count INTO expected_routes
    FROM developer_docs.doc_releases AS releases
    WHERE releases.version = release_version;

    SELECT count(*) INTO actual_routes
    FROM developer_docs.doc_routes AS routes
    WHERE routes.version = release_version;

    IF expected_routes IS NULL OR actual_routes > expected_routes THEN
      RAISE EXCEPTION 'routes for release % are immutable after publication', release_version;
    END IF;
  END LOOP;
  RETURN NULL;
END;
$$;

CREATE OR REPLACE FUNCTION developer_docs.validate_document_release_manifest()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
  actual_routes INTEGER;
  actual_snapshots INTEGER;
BEGIN
  SELECT count(*), count(DISTINCT content_hash)
    INTO actual_routes, actual_snapshots
  FROM developer_docs.doc_routes
  WHERE version = NEW.version;

  IF actual_routes <> NEW.route_count
    OR actual_snapshots <> NEW.unique_snapshot_count THEN
    RAISE EXCEPTION 'release % does not match its manifest counts', NEW.version;
  END IF;
  RETURN NEW;
END;
$$;

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_trigger
    WHERE tgname = 'doc_snapshots_are_immutable'
      AND tgrelid = 'developer_docs.doc_snapshots'::regclass
  ) THEN
    CREATE TRIGGER doc_snapshots_are_immutable
      BEFORE UPDATE OR DELETE ON developer_docs.doc_snapshots
      FOR EACH ROW
      EXECUTE FUNCTION developer_docs.reject_immutable_document_mutation();
  END IF;
  IF NOT EXISTS (
    SELECT 1
    FROM pg_trigger
    WHERE tgname = 'doc_releases_are_immutable'
      AND tgrelid = 'developer_docs.doc_releases'::regclass
  ) THEN
    CREATE TRIGGER doc_releases_are_immutable
      BEFORE UPDATE OR DELETE ON developer_docs.doc_releases
      FOR EACH ROW
      EXECUTE FUNCTION developer_docs.reject_immutable_document_mutation();
  END IF;
  IF NOT EXISTS (
    SELECT 1
    FROM pg_trigger
    WHERE tgname = 'doc_routes_are_immutable'
      AND tgrelid = 'developer_docs.doc_routes'::regclass
  ) THEN
    CREATE TRIGGER doc_routes_are_immutable
      BEFORE UPDATE OR DELETE ON developer_docs.doc_routes
      FOR EACH ROW
      EXECUTE FUNCTION developer_docs.reject_immutable_document_mutation();
  END IF;
  IF NOT EXISTS (
    SELECT 1
    FROM pg_trigger
    WHERE tgname = 'doc_route_insert_capacity'
      AND tgrelid = 'developer_docs.doc_routes'::regclass
  ) THEN
    CREATE TRIGGER doc_route_insert_capacity
      AFTER INSERT ON developer_docs.doc_routes
      REFERENCING NEW TABLE AS inserted_routes
      FOR EACH STATEMENT
      EXECUTE FUNCTION developer_docs.enforce_document_route_capacity();
  END IF;
  IF NOT EXISTS (
    SELECT 1
    FROM pg_trigger
    WHERE tgname = 'doc_release_manifest_is_complete'
      AND tgrelid = 'developer_docs.doc_releases'::regclass
  ) THEN
    CREATE CONSTRAINT TRIGGER doc_release_manifest_is_complete
      AFTER INSERT ON developer_docs.doc_releases
      DEFERRABLE INITIALLY DEFERRED
      FOR EACH ROW
      EXECUTE FUNCTION developer_docs.validate_document_release_manifest();
  END IF;
END;
$$;
