import { describe, expect, it } from 'vitest';

import {
  activeUnionVariant,
  defaultValueForSchema,
  enumValue,
  enumVariantOf,
  isEnumMarkerValue,
  isRawJsonSchema,
  normalizeArgs,
  resolveRef,
  schemaLabel,
  typeLookupFrom,
  valueMatchesSchema,
} from './args-form-model';
import type { FieldSchema, ParamSchema, TypeSchema } from './worker-protocol';

const types: Record<string, TypeSchema> = {
  'user.Color': { kind: 'enum', values: ['Red', 'Green', 'Blue'] },
  'user.Nested': {
    kind: 'class',
    fields: [{ name: 'x', schema: { type: 'int' } }],
  },
  'user.Person': {
    kind: 'class',
    fields: [
      { name: 'name', schema: { type: 'string' } },
      { name: 'age', schema: { type: 'optional', inner: { type: 'int' } } },
      { name: 'color', schema: { type: 'ref', name: 'user.Color' } },
      { name: 'nested', schema: { type: 'ref', name: 'user.Nested' } },
    ],
  },
  'user.Tree': {
    kind: 'class',
    fields: [
      { name: 'value', schema: { type: 'int' } },
      {
        name: 'children',
        schema: { type: 'list', item: { type: 'ref', name: 'user.Tree' } },
      },
    ],
  },
  // Recursive alias: type JSON = string | JSON[]
  'user.JSON': {
    kind: 'alias',
    schema: {
      type: 'union',
      variants: [
        { type: 'string' },
        { type: 'list', item: { type: 'ref', name: 'user.JSON' } },
      ],
    },
  },
  // Pathological pure-ref cycle the extractor shouldn't emit.
  'user.CycleA': { kind: 'alias', schema: { type: 'ref', name: 'user.CycleB' } },
  'user.CycleB': { kind: 'alias', schema: { type: 'ref', name: 'user.CycleA' } },
};
const lookup = typeLookupFrom(types);

const colorRef: FieldSchema = { type: 'ref', name: 'user.Color' };
const personRef: FieldSchema = { type: 'ref', name: 'user.Person' };
const danglingRef: FieldSchema = { type: 'ref', name: 'user.Missing' };

describe('enum markers', () => {
  it('builds and recognizes the wire marker', () => {
    const v = enumValue('user.Color', 'Red');
    expect(v).toEqual({ $baml: { enum: 'user.Color', value: 'Red' } });
    expect(isEnumMarkerValue(v)).toBe(true);
    expect(enumVariantOf(v)).toBe('Red');
  });

  it('falls back to bare strings from hand-edited JSON', () => {
    expect(enumVariantOf('Green')).toBe('Green');
    expect(isEnumMarkerValue('Green')).toBe(false);
    expect(isEnumMarkerValue({ $baml: { type: 'user.Person' } })).toBe(false);
  });
});

describe('resolveRef', () => {
  it('resolves classes and enums from the table', () => {
    expect(resolveRef('user.Color', lookup)).toEqual({
      kind: 'enum',
      name: 'user.Color',
      values: ['Red', 'Green', 'Blue'],
    });
    expect(resolveRef('user.Nested', lookup)).toMatchObject({
      kind: 'class',
      name: 'user.Nested',
    });
  });

  it('resolves a recursive alias to its target schema', () => {
    expect(resolveRef('user.JSON', lookup)).toMatchObject({ kind: 'schema' });
  });

  it('returns undefined for dangling names and pure-ref cycles', () => {
    expect(resolveRef('user.Missing', lookup)).toBeUndefined();
    expect(resolveRef('user.CycleA', lookup)).toBeUndefined();
  });
});

describe('defaultValueForSchema', () => {
  it('seeds primitives with zero values', () => {
    expect(defaultValueForSchema({ type: 'string' }, lookup)).toBe('');
    expect(defaultValueForSchema({ type: 'int' }, lookup)).toBe(0);
    expect(defaultValueForSchema({ type: 'bool' }, lookup)).toBe(false);
    expect(
      defaultValueForSchema(
        { type: 'optional', inner: { type: 'int' } },
        lookup,
      ),
    ).toBeNull();
    expect(
      defaultValueForSchema({ type: 'list', item: { type: 'int' } }, lookup),
    ).toEqual([]);
    expect(
      defaultValueForSchema(
        { type: 'map', key: { type: 'string' }, value: { type: 'int' } },
        lookup,
      ),
    ).toEqual({});
  });

  it('seeds a class one level deep: marker + field defaults, nested class refs marker-only', () => {
    expect(defaultValueForSchema(personRef, lookup)).toEqual({
      $baml: { type: 'user.Person' },
      name: '',
      age: null,
      color: { $baml: { enum: 'user.Color', value: 'Red' } },
      nested: { $baml: { type: 'user.Nested' } },
    });
  });

  it('seeds enums with their first variant as a marker', () => {
    expect(defaultValueForSchema(colorRef, lookup)).toEqual(
      enumValue('user.Color', 'Red'),
    );
  });

  it('seeds enum-variant params with their fixed variant', () => {
    expect(
      defaultValueForSchema(
        { type: 'enumVariant', name: 'user.Status', value: 'Active' },
        lookup,
      ),
    ).toEqual(enumValue('user.Status', 'Active'));
  });

  it('seeds recursive types without blowing up', () => {
    expect(defaultValueForSchema({ type: 'ref', name: 'user.Tree' }, lookup))
      .toEqual({
        $baml: { type: 'user.Tree' },
        value: 0,
        children: [],
      });
    // Recursive alias: first union variant is string.
    expect(
      defaultValueForSchema({ type: 'ref', name: 'user.JSON' }, lookup),
    ).toBe('');
  });

  it('seeds literals with the literal value; dangling refs and cycles with null', () => {
    expect(defaultValueForSchema({ type: 'literal', value: 42 }, lookup)).toBe(
      42,
    );
    expect(defaultValueForSchema(danglingRef, lookup)).toBeNull();
    expect(
      defaultValueForSchema({ type: 'ref', name: 'user.CycleA' }, lookup),
    ).toBeNull();
  });

  it('seeds unions with the first variant and never returns undefined', () => {
    expect(
      defaultValueForSchema(
        { type: 'union', variants: [{ type: 'int' }, { type: 'string' }] },
        lookup,
      ),
    ).toBe(0);
    // Unknown tag from a newer binary: null (JSON-representable), not
    // undefined (silently dropped by JSON.stringify).
    expect(
      defaultValueForSchema({ type: 'wormhole' } as unknown as FieldSchema, lookup),
    ).toBeNull();
  });
});

describe('valueMatchesSchema / activeUnionVariant', () => {
  it('distinguishes primitives structurally', () => {
    expect(valueMatchesSchema('hi', { type: 'string' }, lookup)).toBe(true);
    expect(valueMatchesSchema(1.5, { type: 'int' }, lookup)).toBe(false);
    expect(valueMatchesSchema(1.5, { type: 'float' }, lookup)).toBe(true);
    expect(valueMatchesSchema(null, { type: 'null' }, lookup)).toBe(true);
  });

  it('matches enum refs by marker name and membership', () => {
    expect(
      valueMatchesSchema(enumValue('user.Color', 'Red'), colorRef, lookup),
    ).toBe(true);
    expect(
      valueMatchesSchema(enumValue('user.Other', 'Red'), colorRef, lookup),
    ).toBe(false);
    expect(valueMatchesSchema('Blue', colorRef, lookup)).toBe(true);
    expect(valueMatchesSchema('Magenta', colorRef, lookup)).toBe(false);
  });

  it('separates class values from maps by the $baml marker', () => {
    const mapSchema: FieldSchema = {
      type: 'map',
      key: { type: 'string' },
      value: { type: 'int' },
    };
    const instance = { $baml: { type: 'user.Person' }, name: 'A' };
    expect(valueMatchesSchema(instance, personRef, lookup)).toBe(true);
    expect(valueMatchesSchema(instance, mapSchema, lookup)).toBe(false);
    expect(valueMatchesSchema({ a: 1 }, mapSchema, lookup)).toBe(true);
    // A markerless object could be either; class accepts it.
    expect(valueMatchesSchema({ a: 1 }, personRef, lookup)).toBe(true);
    expect(
      valueMatchesSchema({ $baml: { type: 'user.Other' } }, personRef, lookup),
    ).toBe(false);
  });

  it('matches recursive aliases and admits anything for dangling refs', () => {
    const jsonRef: FieldSchema = { type: 'ref', name: 'user.JSON' };
    expect(valueMatchesSchema('text', jsonRef, lookup)).toBe(true);
    expect(valueMatchesSchema(['a', ['b']], jsonRef, lookup)).toBe(true);
    expect(valueMatchesSchema(7, jsonRef, lookup)).toBe(false);
    expect(valueMatchesSchema(7, danglingRef, lookup)).toBe(true);
  });

  it('selects the union variant the value inhabits', () => {
    const variants: FieldSchema[] = [
      { type: 'int' },
      { type: 'string' },
      { type: 'list', item: { type: 'int' } },
      colorRef,
    ];
    expect(activeUnionVariant(3, variants, lookup)).toBe(0);
    expect(activeUnionVariant('x', variants, lookup)).toBe(1);
    expect(activeUnionVariant([1], variants, lookup)).toBe(2);
    expect(
      activeUnionVariant(enumValue('user.Color', 'Blue'), variants, lookup),
    ).toBe(3);
    expect(activeUnionVariant(true, variants, lookup)).toBe(-1);
  });

  it('first-match detection is not variant-faithful for overlapping unions', () => {
    // Pins the P0.3 premise: the float variant's default (0) detects as int,
    // so the widget must honor the user's explicit choice instead of
    // re-detecting (see UnionField).
    const variants: FieldSchema[] = [{ type: 'int' }, { type: 'float' }];
    const floatDefault = defaultValueForSchema(variants[1], lookup);
    expect(activeUnionVariant(floatDefault, variants, lookup)).toBe(0);
    expect(valueMatchesSchema(floatDefault, variants[1], lookup)).toBe(true);
  });

  it('treats optional as null-or-inner', () => {
    const opt: FieldSchema = { type: 'optional', inner: { type: 'int' } };
    expect(valueMatchesSchema(null, opt, lookup)).toBe(true);
    expect(valueMatchesSchema(2, opt, lookup)).toBe(true);
    expect(valueMatchesSchema('x', opt, lookup)).toBe(false);
  });
});

describe('schemaLabel', () => {
  it('collapses FQNs and composes container labels', () => {
    expect(schemaLabel(colorRef)).toBe('Color');
    expect(schemaLabel(personRef)).toBe('Person');
    expect(schemaLabel({ type: 'list', item: personRef })).toBe('Person[]');
    expect(
      schemaLabel({ type: 'enumVariant', name: 'user.Status', value: 'Active' }),
    ).toBe('Status.Active');
    expect(
      schemaLabel({ type: 'optional', inner: { type: 'string' } }),
    ).toBe('string?');
    expect(
      schemaLabel({
        type: 'union',
        variants: [{ type: 'int' }, { type: 'string' }],
      }),
    ).toBe('int | string');
    expect(schemaLabel({ type: 'media', kind: 'image' })).toBe('image');
    expect(
      schemaLabel({ type: 'unsupported', display: 'callback' }),
    ).toBe('callback');
  });
});

describe('isRawJsonSchema', () => {
  it('sends unsupported, media, dangling refs, and unknown tags to raw JSON', () => {
    expect(isRawJsonSchema({ type: 'unsupported', display: 'T' }, lookup)).toBe(
      true,
    );
    expect(isRawJsonSchema({ type: 'media', kind: 'pdf' }, lookup)).toBe(true);
    expect(isRawJsonSchema(danglingRef, lookup)).toBe(true);
    expect(
      isRawJsonSchema({ type: 'ref', name: 'user.CycleA' }, lookup),
    ).toBe(true);
    expect(
      isRawJsonSchema({ type: 'wormhole' } as unknown as FieldSchema, lookup),
    ).toBe(true);
    expect(isRawJsonSchema(personRef, lookup)).toBe(false);
    expect(isRawJsonSchema(colorRef, lookup)).toBe(false);
    expect(isRawJsonSchema({ type: 'ref', name: 'user.JSON' }, lookup)).toBe(
      false,
    );
    expect(isRawJsonSchema({ type: 'string' }, lookup)).toBe(false);
  });
});

describe('normalizeArgs', () => {
  const params: ParamSchema[] = [
    { name: 'c', hasDefault: false, schema: colorRef },
    { name: 'p', hasDefault: false, schema: personRef },
    {
      name: 'list',
      hasDefault: false,
      schema: { type: 'list', item: colorRef },
    },
  ];

  it('rewrites bare enum strings that name a valid variant', () => {
    expect(normalizeArgs({ c: 'Red' }, params, lookup)).toEqual({
      c: enumValue('user.Color', 'Red'),
    });
    // Inside containers too.
    expect(normalizeArgs({ list: ['Red', 'Blue'] }, params, lookup)).toEqual({
      list: [enumValue('user.Color', 'Red'), enumValue('user.Color', 'Blue')],
    });
    // Not when the string is not a variant.
    const invalid = { c: 'Magenta' };
    expect(normalizeArgs(invalid, params, lookup)).toBe(invalid);
  });

  it('injects marker and missing-field defaults into markerless class objects', () => {
    expect(
      normalizeArgs({ p: { name: 'Ada', color: 'Green' } }, params, lookup),
    ).toEqual({
      p: {
        $baml: { type: 'user.Person' },
        name: 'Ada',
        color: enumValue('user.Color', 'Green'),
        age: null,
        nested: { $baml: { type: 'user.Nested' } },
      },
    });
  });

  it('overwrites a junk non-marker $baml key with the injected marker', () => {
    // Raw editing can leave `$baml: 5` behind; spreading it over the marker
    // would silently keep the value untyped while widgets render it typed.
    expect(
      normalizeArgs({ p: { $baml: 5, name: 'x' } }, params, lookup),
    ).toMatchObject({
      p: { $baml: { type: 'user.Person' }, name: 'x' },
    });
  });

  it('leaves marker-carrying objects structurally alone (normalizes present fields only)', () => {
    const args = {
      p: { $baml: { type: 'user.Person' }, color: 'Blue' },
    };
    expect(normalizeArgs(args, params, lookup)).toEqual({
      p: { $baml: { type: 'user.Person' }, color: enumValue('user.Color', 'Blue') },
    });
    // Idempotent: already-typed input comes back by reference.
    const typed = {
      c: enumValue('user.Color', 'Red'),
      p: { $baml: { type: 'user.Person' }, name: 'Ada' },
    };
    expect(normalizeArgs(typed, params, lookup)).toBe(typed);
  });

  it('preserves surplus keys and values without schemas', () => {
    const args = { c: enumValue('user.Color', 'Red'), extra: { a: 1 } };
    expect(normalizeArgs(args, params, lookup)).toBe(args);
  });
});
