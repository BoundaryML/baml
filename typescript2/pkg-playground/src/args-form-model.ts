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

/** Marker-only class instance — used as the finite cut point for a required
 *  recursive class path that cannot be populated to arbitrary depth. */
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
 *  Required acyclic class refs are populated recursively so every default a
 *  typed widget displays is also present in the serialized value. A class ref
 *  already active on the current path becomes marker-only, which is the
 *  finite cut point for an irreducibly required recursive type. Optional and
 *  container branches terminate naturally at null/empty values. */
export function defaultValueForSchema(
  schema: FieldSchema,
  lookup: TypeLookup,
): unknown {
  return defaultValue(schema, lookup, new Set(), new Set());
}

function defaultValue(
  schema: FieldSchema,
  lookup: TypeLookup,
  classPath: Set<string>,
  /** Ref names already hopped through at this level (alias chains); guards
   *  pathological pure-ref cycles the extractor shouldn't emit. */
  aliasPath: Set<string>,
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
      if (aliasPath.has(schema.name)) return null;
      const resolved = resolveRef(schema.name, lookup);
      if (resolved === undefined) return null;
      if (resolved.kind === 'enum') {
        return resolved.values.length > 0
          ? enumValue(resolved.name, resolved.values[0])
          : null;
      }
      if (resolved.kind === 'schema') {
        const next = new Set(aliasPath);
        next.add(schema.name);
        return defaultValue(resolved.schema, lookup, classPath, next);
      }
      if (classPath.has(resolved.name)) {
        return classMarkerValue(resolved.name);
      }
      const nextClassPath = new Set(classPath);
      nextClassPath.add(resolved.name);
      const obj = classMarkerValue(resolved.name);
      for (const field of resolved.fields) {
        obj[field.name] = defaultValue(
          field.schema,
          lookup,
          nextClassPath,
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
        ? defaultValue(schema.variants[0], lookup, classPath, aliasPath)
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
 *  pick the active union variant and by schema reconciliation
 *  ([`reconcileArgs`]) to route parsed raw JSON to the right variant.
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
 * Reconcile serialized args with the current function schema so the values
 * rendered by typed widgets are exactly the values that will be encoded.
 *
 * Compatible user values and surplus raw-JSON keys are preserved. Missing
 * required parameters/fields receive schema defaults, incompatible values are
 * replaced with defaults, and typed class/enum markers are normalized.
 * Defaulted function parameters remain absent so the runtime evaluates their
 * declared BAML defaults. Returns the input reference when no change is
 * required, allowing callers to avoid redundant state writes.
 */
export function reconcileArgs(
  args: Record<string, unknown>,
  params: ParamSchema[],
  lookup: TypeLookup,
): Record<string, unknown> {
  const paramsByName = new Map(params.map((param) => [param.name, param]));
  let reconciled = mapObject(args, (key, value) => {
    const param = paramsByName.get(key);
    return param === undefined
      ? value
      : reconcileValue(value, param.schema, lookup, new Set(), new Set());
  });

  for (const param of params) {
    if (!(param.name in reconciled) && !param.hasDefault) {
      if (reconciled === args) reconciled = { ...args };
      reconciled[param.name] = defaultValueForSchema(param.schema, lookup);
    }
  }

  return reconciled;
}

function reconcileValue(
  value: unknown,
  schema: FieldSchema,
  lookup: TypeLookup,
  /** Ref names hopped through without descending into a child value. */
  aliasPath: Set<string>,
  /** Class names active on the current value path. */
  classPath: Set<string>,
): unknown {
  if (!valueMatchesSchema(value, schema, lookup)) {
    return defaultValue(schema, lookup, classPath, aliasPath);
  }

  switch (schema.type) {
    case 'enumVariant':
      return typeof value === 'string' && value === schema.value
        ? enumValue(schema.name, value)
        : value;
    case 'ref': {
      if (aliasPath.has(schema.name)) return value;
      const resolved = resolveRef(schema.name, lookup);
      if (resolved === undefined) return value;
      if (resolved.kind === 'enum') {
        return typeof value === 'string' && resolved.values.includes(value)
          ? enumValue(resolved.name, value)
          : value;
      }
      if (resolved.kind === 'schema') {
        const next = new Set(aliasPath);
        next.add(schema.name);
        return reconcileValue(
          value,
          resolved.schema,
          lookup,
          next,
          classPath,
        );
      }
      if (!isPlainObject(value) || isEnumMarkerValue(value)) return value;
      const marker = classMarkerOf(value);
      const recursiveClass = classPath.has(resolved.name);
      const nextClassPath = new Set(classPath);
      nextClassPath.add(resolved.name);
      const fieldSchemas = new Map(
        resolved.fields.map((f) => [f.name, f.schema]),
      );
      let reconciled = mapObject(value, (key, fieldValue) => {
        const fieldSchema = fieldSchemas.get(key);
        return fieldSchema === undefined
          ? fieldValue
          : reconcileValue(
              fieldValue,
              fieldSchema,
              lookup,
              new Set(),
              nextClassPath,
            );
      });
      if (marker !== resolved.name) {
        // Marker spread last: a junk non-marker `$baml` key from raw editing
        // must be overwritten, not preserved over the injected marker.
        reconciled = {
          ...reconciled,
          ...classMarkerValue(resolved.name),
        };
      }
      if (!recursiveClass) {
        for (const field of resolved.fields) {
          if (!(field.name in reconciled)) {
            if (reconciled === value) reconciled = { ...value };
            reconciled[field.name] = defaultValue(
              field.schema,
              lookup,
              nextClassPath,
              new Set(),
            );
          }
        }
      }
      return reconciled;
    }
    case 'list': {
      if (!Array.isArray(value)) return value;
      const items = value.map((item) =>
        reconcileValue(item, schema.item, lookup, new Set(), classPath),
      );
      return items.some((item, i) => item !== value[i]) ? items : value;
    }
    case 'map': {
      if (!valueMatchesSchema(value, schema, lookup)) return value;
      return mapObject(value as Record<string, unknown>, (_key, entry) =>
        reconcileValue(entry, schema.value, lookup, new Set(), classPath),
      );
    }
    case 'optional':
      return value === null
        ? value
        : reconcileValue(
            value,
            schema.inner,
            lookup,
            aliasPath,
            classPath,
          );
    case 'union': {
      const active = schema.variants.findIndex((v) =>
        valueMatchesSchema(value, v, lookup),
      );
      return active === -1
        ? defaultValue(schema, lookup, classPath, aliasPath)
        : reconcileValue(
            value,
            schema.variants[active],
            lookup,
            aliasPath,
            classPath,
          );
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
