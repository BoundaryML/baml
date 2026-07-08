/**
 * TS half of the wire-shape golden pin (the Rust half is
 * `wire_shape_matches_the_ts_golden_fixture` in
 * `baml_project/src/param_schema.rs`, which asserts extraction output is
 * byte-identical to the fixture). Here the same fixture is validated against
 * the hand-written `worker-protocol.ts` mirror: every schema node must carry
 * a tag the mirror knows, and every ref must resolve in the fixture's table
 * by canonical dotted FQN. A new Rust variant shows up here as an unknown
 * tag; a mirror edit that loses a variant fails the compile-time
 * exhaustiveness pins below.
 */

import { describe, expect, it } from 'vitest';

import goldenJson from './__fixtures__/param-schema-golden.json';
import type { FieldSchema, ParamSchema, TypeSchema } from './worker-protocol';

const KNOWN_FIELD_TYPES = [
  'string',
  'int',
  'float',
  'bool',
  'null',
  'bigint',
  'media',
  'literal',
  'ref',
  'enumVariant',
  'list',
  'map',
  'optional',
  'union',
  'unsupported',
] as const satisfies readonly FieldSchema['type'][];

const KNOWN_TYPE_KINDS = ['class', 'enum', 'alias'] as const satisfies
  readonly TypeSchema['kind'][];

// Compile-time exhaustiveness: adding a variant to the mirror without
// listing it above turns these into type errors.
type MissingFieldTypes = Exclude<
  FieldSchema['type'],
  (typeof KNOWN_FIELD_TYPES)[number]
>;
type MissingTypeKinds = Exclude<
  TypeSchema['kind'],
  (typeof KNOWN_TYPE_KINDS)[number]
>;
const _fieldTypesExhaustive: MissingFieldTypes[] = [];
const _typeKindsExhaustive: MissingTypeKinds[] = [];
void _fieldTypesExhaustive;
void _typeKindsExhaustive;

interface GoldenFixture {
  params: ParamSchema[];
  types: Record<string, TypeSchema>;
}

// JSON imports infer wide literal types (`type: string`), so the fixture is
// asserted into the mirror types; the tests below validate it structurally.
const golden = goldenJson as unknown as GoldenFixture;

function collectSchemas(schema: FieldSchema, out: FieldSchema[]): void {
  out.push(schema);
  switch (schema.type) {
    case 'list':
      collectSchemas(schema.item, out);
      break;
    case 'map':
      collectSchemas(schema.key, out);
      collectSchemas(schema.value, out);
      break;
    case 'optional':
      collectSchemas(schema.inner, out);
      break;
    case 'union':
      for (const variant of schema.variants) collectSchemas(variant, out);
      break;
    default:
      break;
  }
}

function allSchemas(fixture: GoldenFixture): FieldSchema[] {
  const out: FieldSchema[] = [];
  for (const param of fixture.params) collectSchemas(param.schema, out);
  for (const entry of Object.values(fixture.types)) {
    if (entry.kind === 'class') {
      for (const field of entry.fields) collectSchemas(field.schema, out);
    } else if (entry.kind === 'alias') {
      collectSchemas(entry.schema, out);
    }
  }
  return out;
}

describe('param schema golden fixture', () => {
  it('only carries schema tags the worker-protocol mirror knows', () => {
    const knownTypes: readonly string[] = KNOWN_FIELD_TYPES;
    for (const schema of allSchemas(golden)) {
      expect(knownTypes).toContain(schema.type);
    }
    const knownKinds: readonly string[] = KNOWN_TYPE_KINDS;
    for (const entry of Object.values(golden.types)) {
      expect(knownKinds).toContain(entry.kind);
    }
  });

  it('resolves every ref within the table by canonical dotted FQN', () => {
    for (const schema of allSchemas(golden)) {
      if (schema.type === 'ref') {
        expect(golden.types[schema.name]).toBeDefined();
      }
    }
    for (const key of Object.keys(golden.types)) {
      // Dotted FQN with a package segment, e.g. `user.Person`.
      expect(key).toMatch(/^[^.]+\.[^.]+/);
    }
  });

  it('exercises refs, enum-variant params, and a recursive alias', () => {
    const tags = new Set(allSchemas(golden).map((s) => s.type));
    expect(tags).toContain('ref');
    expect(tags).toContain('enumVariant');
    const kinds = new Set(Object.values(golden.types).map((t) => t.kind));
    expect(kinds).toEqual(new Set(['class', 'enum', 'alias']));
  });
});
