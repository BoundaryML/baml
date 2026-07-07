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
 */

import type { FieldSchema } from './worker-protocol';

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

function isPlainObject(value: unknown): value is Record<string, unknown> {
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

/** A sensible zero value for a schema node, used to seed new list rows,
 *  freshly-enabled optionals, and union-variant switches. */
export function defaultValueForSchema(schema: FieldSchema): unknown {
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
    case 'enum':
      return schema.values.length > 0
        ? enumValue(schema.name, schema.values[0])
        : null;
    case 'class': {
      if (schema.recursive) return null;
      const obj: Record<string, unknown> = {
        $baml: { type: schema.name },
      };
      for (const field of schema.fields) {
        obj[field.name] = defaultValueForSchema(field.schema);
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
        ? defaultValueForSchema(schema.variants[0])
        : null;
    case 'media':
    case 'unsupported':
      return null;
  }
}

/** Loose structural check: does `value` plausibly inhabit `schema`? Used to
 *  pick the active union variant and to decide whether parsed raw JSON can
 *  hydrate a widget. Marker-carrying values are matched by name; plain
 *  values by JS shape. */
export function valueMatchesSchema(
  value: unknown,
  schema: FieldSchema,
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
    case 'enum': {
      const variant = enumVariantOf(value);
      if (variant === undefined) return false;
      if (isEnumMarkerValue(value) && value.$baml.enum !== schema.name) {
        return false;
      }
      return schema.values.includes(variant);
    }
    case 'class': {
      if (!isPlainObject(value) || isEnumMarkerValue(value)) return false;
      const marker = classMarkerOf(value);
      return marker === undefined || marker === schema.name;
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
      return value === null || valueMatchesSchema(value, schema.inner);
    case 'union':
      return schema.variants.some((v) => valueMatchesSchema(value, v));
    case 'media':
    case 'unsupported':
      return true;
  }
}

/** Index of the union variant `value` currently inhabits, or -1. */
export function activeUnionVariant(
  value: unknown,
  variants: FieldSchema[],
): number {
  return variants.findIndex((v) => valueMatchesSchema(value, v));
}

/** Short human label for a schema node — union variant chips, list-item
 *  headers. Class/enum FQNs collapse to their last segment. */
export function schemaLabel(schema: FieldSchema): string {
  switch (schema.type) {
    case 'media':
      return schema.kind;
    case 'literal':
      return JSON.stringify(schema.value) ?? 'literal';
    case 'enum':
    case 'class':
      return schema.name.split('.').pop() ?? schema.name;
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
 *  typed widget. (Optional wrappers still get the null toggle around it.) */
export function isRawJsonSchema(schema: FieldSchema): boolean {
  return (
    schema.type === 'unsupported' ||
    schema.type === 'media' ||
    (schema.type === 'class' && schema.recursive)
  );
}
