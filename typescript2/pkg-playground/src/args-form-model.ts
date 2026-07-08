/**
 * Pure logic for the dynamic args form (see ArgsForm.tsx for the widgets).
 *
 * The form's output is a plain JS object that round-trips through
 * `JSON.stringify` → the `argsJson` string → `JSON.parse` → `encodeRunArgs`.
 * Two `$baml` markers make typed values survive that pipeline:
 *
 * - class:  `{ $baml: { type: 'user.Person' }, ...fields }` — required for
 *   class values in *nested* positions (list/map elements, class fields),
 *   where the runtime does not promote bare maps to instances.
 * - enum:   `{ $baml: { enum: 'user.Color', value: 'Red' } }` — required for
 *   expr functions; plain strings are never coerced to enum variants on the
 *   args path.
 *
 * Names are the canonical dotted FQN the engine registers (`user.shapes.Foo`),
 * passed verbatim by the encoder.
 *
 * Named types arrive as `{ type: 'ref' }` nodes into the per-project table
 * (`ProjectUpdate.types`); every function here resolves refs through a
 * `TypeLookup`. A dangling ref — mid-edit inconsistency, or a table from an
 * older binary — routes to the raw-JSON path, as do unknown schema tags from
 * a newer binary.
 */

import type {
  FieldSchema,
  FieldSchemaField,
  ParamSchema,
  TypeSchema,
} from './worker-protocol';

/** Resolves a canonical dotted FQN against the project's type table. */
export type TypeLookup = (name: string) => TypeSchema | undefined;

/** Adapt a `ProjectUpdate.types` table (possibly absent) to a [`TypeLookup`]. */
export function typeLookupFrom(
  types: Record<string, TypeSchema> | undefined,
): TypeLookup {
  return (name) => types?.[name];
}

export interface EnumMarkerValue {
  $baml: { enum: string; value: string };
}

export function enumValue(name: string, variant: string): EnumMarkerValue {
  return { $baml: { enum: name, value: variant } };
}

export function isEnumMarkerValue(value: unknown): value is EnumMarkerValue {
  if (typeof value !== 'object' || value === null) return false;
  const marker = (value as Record<string, unknown>)['$baml'];
  return (
    typeof marker === 'object' &&
    marker !== null &&
    typeof (marker as Record<string, unknown>).enum === 'string' &&
    typeof (marker as Record<string, unknown>).value === 'string'
  );
}

/** The selected variant of an enum-schema value: reads the `$baml` marker,
 *  accepting a bare string (e.g. hand-edited raw JSON) as a fallback. */
export function enumVariantOf(value: unknown): string | undefined {
  if (isEnumMarkerValue(value)) return value.$baml.value;
  if (typeof value === 'string') return value;
  return undefined;
}

export function isPlainObject(
  value: unknown,
): value is Record<string, unknown> {
  return (
    typeof value === 'object' && value !== null && !Array.isArray(value)
  );
}

function classMarkerOf(value: unknown): string | undefined {
  if (!isPlainObject(value)) return undefined;
  const marker = value['$baml'];
  if (isPlainObject(marker) && typeof marker.type === 'string') {
    return marker.type;
  }
  return undefined;
}

/** Marker-only class instance — what nested class refs seed; fields fill in
 *  as the user edits them (`ClassSection` injects the marker on every edit). */
export function classMarkerValue(name: string): Record<string, unknown> {
  return { $baml: { type: name } };
}

/** A ref resolved through the type table, with alias chains flattened:
 *  - class/enum: the named type, under its own canonical FQN;
 *  - schema: an alias's target (render/match that schema in place);
 *  - undefined: dangling name or a pure ref cycle → raw-JSON fallback. */
export type ResolvedRef =
  | { kind: 'class'; name: string; fields: FieldSchemaField[] }
  | { kind: 'enum'; name: string; values: string[] }
  | { kind: 'schema'; schema: FieldSchema }
  | undefined;

export function resolveRef(name: string, lookup: TypeLookup): ResolvedRef {
  const seen = new Set<string>();
  let current = name;
  for (;;) {
    if (seen.has(current)) return undefined;
    seen.add(current);
    const entry = lookup(current);
    if (entry === undefined) return undefined;
    if (entry.kind === 'class') {
      return { kind: 'class', name: current, fields: entry.fields };
    }
    if (entry.kind === 'enum') {
      return { kind: 'enum', name: current, values: entry.values };
    }
    if (entry.kind === 'alias') {
      // Flatten bare-ref alias chains so callers land on the real target;
      // any other alias body is a schema to use in place.
      if (entry.schema.type === 'ref') {
        current = entry.schema.name;
        continue;
      }
      return { kind: 'schema', schema: entry.schema };
    }
    // Unknown table-entry kind from a newer binary.
    return undefined;
  }
}

/** A sensible zero value for a schema node, used to seed new list rows,
 *  freshly-enabled optionals, union-variant switches, and empty args.
 *
 *  Seeding depth rule: a class ref expands **one level** — marker plus
 *  defaults for its immediate fields, where a nested class ref seeds
 *  marker-only. Deeper levels fill in as the user opens sections and edits
 *  fields (`ClassSection` injects the marker on edit). Eager deep seeding
 *  would reintroduce the payload blow-up on DAG-shaped class graphs
 *  client-side. */
export function defaultValueForSchema(
  schema: FieldSchema,
  lookup: TypeLookup,
): unknown {
  return defaultValue(schema, lookup, 0, new Set());
}

function defaultValue(
  schema: FieldSchema,
  lookup: TypeLookup,
  classDepth: number,
  /** Ref names already hopped through at this level (alias chains); guards
   *  pathological pure-ref cycles the extractor shouldn't emit. */
  seen: Set<string>,
): unknown {
  switch (schema.type) {
    case 'string':
      return '';
    case 'int':
    case 'float':
    case 'bigint':
      return 0;
    case 'bool':
      return false;
    case 'null':
      return null;
    case 'literal':
      return schema.value;
    case 'enumVariant':
      return enumValue(schema.name, schema.value);
    case 'ref': {
      if (seen.has(schema.name)) return null;
      const resolved = resolveRef(schema.name, lookup);
      if (resolved === undefined) return null;
      if (resolved.kind === 'enum') {
        return resolved.values.length > 0
          ? enumValue(resolved.name, resolved.values[0])
          : null;
      }
      if (resolved.kind === 'schema') {
        const next = new Set(seen);
        next.add(schema.name);
        return defaultValue(resolved.schema, lookup, classDepth, next);
      }
      if (classDepth >= 1) return classMarkerValue(resolved.name);
      const obj = classMarkerValue(resolved.name);
      for (const field of resolved.fields) {
        obj[field.name] = defaultValue(
          field.schema,
          lookup,
          classDepth + 1,
          new Set(),
        );
      }
      return obj;
    }
    case 'list':
      return [];
    case 'map':
      return {};
    case 'optional':
      return null;
    case 'union':
      return schema.variants.length > 0
        ? defaultValue(schema.variants[0], lookup, classDepth, seen)
        : null;
    case 'media':
    case 'unsupported':
      return null;
    default:
      // Unknown tag from a newer binary — raw-JSON path; never `undefined`,
      // which JSON.stringify silently drops from seeded args.
      return null;
  }
}

/** Loose structural check: does `value` plausibly inhabit `schema`? Used to
 *  pick the active union variant and by hydration normalization
 *  ([`normalizeArgs`]) to route parsed raw JSON to the right variant.
 *  Marker-carrying values are matched by name; plain values by JS shape.
 *  Raw-JSON nodes (unsupported/media/dangling refs/unknown tags) admit
 *  anything. */
export function valueMatchesSchema(
  value: unknown,
  schema: FieldSchema,
  lookup: TypeLookup,
): boolean {
  return matches(value, schema, lookup, new Set());
}

function matches(
  value: unknown,
  schema: FieldSchema,
  lookup: TypeLookup,
  seen: Set<string>,
): boolean {
  switch (schema.type) {
    case 'string':
      return typeof value === 'string' && !isEnumMarkerValue(value);
    case 'int':
      return typeof value === 'number' && Number.isInteger(value);
    case 'float':
    case 'bigint':
      return typeof value === 'number' || typeof value === 'bigint';
    case 'bool':
      return typeof value === 'boolean';
    case 'null':
      return value === null;
    case 'literal':
      return value === schema.value;
    case 'enumVariant':
      return enumVariantOf(value) === schema.value &&
        (!isEnumMarkerValue(value) || value.$baml.enum === schema.name);
    case 'ref': {
      if (seen.has(schema.name)) return false;
      const resolved = resolveRef(schema.name, lookup);
      if (resolved === undefined) return true; // raw-JSON path admits anything
      if (resolved.kind === 'enum') {
        const variant = enumVariantOf(value);
        if (variant === undefined) return false;
        if (isEnumMarkerValue(value) && value.$baml.enum !== resolved.name) {
          return false;
        }
        return resolved.values.includes(variant);
      }
      if (resolved.kind === 'schema') {
        const next = new Set(seen);
        next.add(schema.name);
        return matches(value, resolved.schema, lookup, next);
      }
      if (!isPlainObject(value) || isEnumMarkerValue(value)) return false;
      const marker = classMarkerOf(value);
      return marker === undefined || marker === resolved.name;
    }
    case 'list':
      return Array.isArray(value);
    case 'map':
      return (
        isPlainObject(value) &&
        classMarkerOf(value) === undefined &&
        !isEnumMarkerValue(value)
      );
    case 'optional':
      return value === null || matches(value, schema.inner, lookup, seen);
    case 'union':
      return schema.variants.some((v) => matches(value, v, lookup, seen));
    case 'media':
    case 'unsupported':
      return true;
    default:
      return true; // unknown tag → raw-JSON path admits anything
  }
}

/** Index of the union variant `value` currently inhabits, or -1. */
export function activeUnionVariant(
  value: unknown,
  variants: FieldSchema[],
  lookup: TypeLookup,
): number {
  return variants.findIndex((v) => valueMatchesSchema(value, v, lookup));
}

/** Short human label for a schema node — union variant chips, list-item
 *  headers. Ref/enum-variant FQNs collapse to their last segment; no table
 *  lookup needed. */
export function schemaLabel(schema: FieldSchema): string {
  switch (schema.type) {
    case 'media':
      return schema.kind;
    case 'literal':
      return JSON.stringify(schema.value) ?? 'literal';
    case 'ref':
      return schema.name.split('.').pop() ?? schema.name;
    case 'enumVariant':
      return `${schema.name.split('.').pop() ?? schema.name}.${schema.value}`;
    case 'list':
      return `${schemaLabel(schema.item)}[]`;
    case 'map':
      return 'map';
    case 'optional':
      return `${schemaLabel(schema.inner)}?`;
    case 'union':
      return schema.variants.map(schemaLabel).join(' | ');
    case 'unsupported':
      return schema.display;
    default:
      return schema.type;
  }
}

/** Whether the form renders this node as a raw-JSON textarea rather than a
 *  typed widget: unsupported/media nodes, dangling refs, and unknown schema
 *  tags. (Optional wrappers still get the null toggle around it.) */
export function isRawJsonSchema(
  schema: FieldSchema,
  lookup: TypeLookup,
): boolean {
  switch (schema.type) {
    case 'unsupported':
    case 'media':
      return true;
    case 'ref': {
      const resolved = resolveRef(schema.name, lookup);
      if (resolved === undefined) return true;
      return resolved.kind === 'schema'
        ? isRawJsonSchema(resolved.schema, lookup)
        : false;
    }
    case 'string':
    case 'int':
    case 'float':
    case 'bool':
    case 'null':
    case 'bigint':
    case 'literal':
    case 'enumVariant':
    case 'list':
    case 'map':
    case 'optional':
    case 'union':
      return false;
    default:
      return true; // unknown tag from a newer binary
  }
}

/**
 * Rewrite wire-untyped values into their typed marker forms so what the
 * widgets display is what actually encodes: bare enum strings (hand-edited
 * raw JSON, pre-marker session memory) become `$baml` enum markers, and
 * markerless class objects get the class marker plus defaults for missing
 * fields. Marker-carrying objects are left structurally alone (only their
 * present fields are normalized) — hydration must not grow already-typed
 * values. Returns the input reference unchanged when nothing needed fixing,
 * so callers can cheaply detect a no-op.
 */
export function normalizeArgs(
  args: Record<string, unknown>,
  params: ParamSchema[],
  lookup: TypeLookup,
): Record<string, unknown> {
  const schemaByName = new Map(params.map((p) => [p.name, p.schema]));
  return mapObject(args, (key, value) => {
    const schema = schemaByName.get(key);
    return schema === undefined
      ? value
      : normalize(value, schema, lookup, new Set());
  });
}

function normalize(
  value: unknown,
  schema: FieldSchema,
  lookup: TypeLookup,
  /** Ref names hopped through without descending into a child value. */
  seen: Set<string>,
): unknown {
  switch (schema.type) {
    case 'enumVariant':
      return typeof value === 'string' && value === schema.value
        ? enumValue(schema.name, value)
        : value;
    case 'ref': {
      if (seen.has(schema.name)) return value;
      const resolved = resolveRef(schema.name, lookup);
      if (resolved === undefined) return value;
      if (resolved.kind === 'enum') {
        return typeof value === 'string' && resolved.values.includes(value)
          ? enumValue(resolved.name, value)
          : value;
      }
      if (resolved.kind === 'schema') {
        const next = new Set(seen);
        next.add(schema.name);
        return normalize(value, resolved.schema, lookup, next);
      }
      if (!isPlainObject(value) || isEnumMarkerValue(value)) return value;
      const fieldSchemas = new Map(
        resolved.fields.map((f) => [f.name, f.schema]),
      );
      const normalized = mapObject(value, (key, fieldValue) => {
        const fieldSchema = fieldSchemas.get(key);
        return fieldSchema === undefined
          ? fieldValue
          : normalize(fieldValue, fieldSchema, lookup, new Set());
      });
      if (classMarkerOf(value) !== undefined) return normalized;
      // Marker spread last: a junk non-marker `$baml` key from raw editing
      // must be overwritten, not preserved over the injected marker.
      const withMarker: Record<string, unknown> = {
        ...normalized,
        ...classMarkerValue(resolved.name),
      };
      for (const field of resolved.fields) {
        if (!(field.name in withMarker)) {
          // Same one-level depth rule as seeding: these fields sit inside a
          // class, so nested class refs default to marker-only.
          withMarker[field.name] = defaultValue(field.schema, lookup, 1, new Set());
        }
      }
      return withMarker;
    }
    case 'list': {
      if (!Array.isArray(value)) return value;
      const items = value.map((item) =>
        normalize(item, schema.item, lookup, new Set()),
      );
      return items.some((item, i) => item !== value[i]) ? items : value;
    }
    case 'map': {
      if (!valueMatchesSchema(value, schema, lookup)) return value;
      return mapObject(value as Record<string, unknown>, (_key, entry) =>
        normalize(entry, schema.value, lookup, new Set()),
      );
    }
    case 'optional':
      return value === null
        ? value
        : normalize(value, schema.inner, lookup, seen);
    case 'union': {
      const active = schema.variants.findIndex((v) =>
        valueMatchesSchema(value, v, lookup),
      );
      return active === -1
        ? value
        : normalize(value, schema.variants[active], lookup, seen);
    }
    default:
      return value;
  }
}

/** Shallow-map an object's values, returning the original reference when
 *  every mapped value is identical. */
function mapObject(
  obj: Record<string, unknown>,
  map: (key: string, value: unknown) => unknown,
): Record<string, unknown> {
  let changed = false;
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(obj)) {
    const next = map(key, value);
    if (next !== value) changed = true;
    out[key] = next;
  }
  return changed ? out : obj;
}
