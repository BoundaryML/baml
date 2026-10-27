import { describe, expect, it } from 'vitest';

import {
  activeUnionVariant,
  defaultValueForSchema,
  enumVariantOf,
  isRawJsonSchema,
  reconcileArgs,
  resolveRef,
  schemaLabel,
  typeLookupFrom,
  valueMatchesSchema,
} from './args-form-model';
import type { FieldSchema, ParamSchema, TypeSchema } from './worker-protocol';

const types: Record<string, TypeSchema> = {
  'user.Color': { kind: 'enum', values: ['Red', 'Green', 'Blue'] },
  // Pathological pure-ref cycle the extractor shouldn't emit.
  'user.CycleA': {
    kind: 'alias',
    schema: { name: 'user.CycleB', type: 'ref' },
  },
  'user.CycleB': {
    kind: 'alias',
    schema: { name: 'user.CycleA', type: 'ref' },
  },
  // Recursive alias: type JSON = string | JSON[]
  'user.JSON': {
    kind: 'alias',
    schema: {
      type: 'union',
      variants: [
        { type: 'string' },
        { item: { name: 'user.JSON', type: 'ref' }, type: 'list' },
      ],
    },
  },
  'user.Nested': {
    fields: [{ name: 'x', schema: { type: 'int' } }],
    kind: 'class',
  },
  'user.Person': {
    fields: [
      { name: 'name', schema: { type: 'string' } },
      { name: 'age', schema: { inner: { type: 'int' }, type: 'optional' } },
      { name: 'color', schema: { name: 'user.Color', type: 'ref' } },
      { name: 'nested', schema: { name: 'user.Nested', type: 'ref' } },
    ],
    kind: 'class',
  },
  'user.RequiredNode': {
    fields: [
      { name: 'value', schema: { type: 'int' } },
      {
        name: 'next',
        schema: { name: 'user.RequiredNode', type: 'ref' },
      },
    ],
    kind: 'class',
  },
  'user.Tree': {
    fields: [
      { name: 'value', schema: { type: 'int' } },
      {
        name: 'children',
        schema: { item: { name: 'user.Tree', type: 'ref' }, type: 'list' },
      },
    ],
    kind: 'class',
  },
};
const lookup = typeLookupFrom(types);

const colorRef: FieldSchema = { name: 'user.Color', type: 'ref' };
const personRef: FieldSchema = { name: 'user.Person', type: 'ref' };
const danglingRef: FieldSchema = { name: 'user.Missing', type: 'ref' };

describe('enumVariantOf', () => {
  it('reads a variant from its name and nothing else', () => {
    expect(enumVariantOf('Green')).toBe('Green');
    expect(enumVariantOf(3)).toBeUndefined();
    expect(enumVariantOf({ value: 'Red' })).toBeUndefined();
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
        { inner: { type: 'int' }, type: 'optional' },
        lookup,
      ),
    ).toBeNull();
    expect(
      defaultValueForSchema({ item: { type: 'int' }, type: 'list' }, lookup),
    ).toEqual([]);
    expect(
      defaultValueForSchema(
        { key: { type: 'string' }, type: 'map', value: { type: 'int' } },
        lookup,
      ),
    ).toEqual({});
  });

  it('seeds required nested classes with every displayed field default', () => {
    expect(defaultValueForSchema(personRef, lookup)).toEqual({
      age: null,
      color: 'Red',
      name: '',
      nested: { x: 0 },
    });
  });

  it('seeds enums with their first variant', () => {
    expect(defaultValueForSchema(colorRef, lookup)).toBe('Red');
  });

  it('seeds enum-variant params with their fixed variant', () => {
    expect(
      defaultValueForSchema(
        { name: 'user.Status', type: 'enumVariant', value: 'Active' },
        lookup,
      ),
    ).toBe('Active');
  });

  it('seeds recursive types without blowing up', () => {
    expect(
      defaultValueForSchema({ name: 'user.Tree', type: 'ref' }, lookup),
    ).toEqual({
      children: [],
      value: 0,
    });
    // Recursive alias: first union variant is string.
    expect(
      defaultValueForSchema({ name: 'user.JSON', type: 'ref' }, lookup),
    ).toBe('');
  });

  it('seeds literals with the literal value; dangling refs and cycles with null', () => {
    expect(defaultValueForSchema({ type: 'literal', value: 42 }, lookup)).toBe(
      42,
    );
    expect(defaultValueForSchema(danglingRef, lookup)).toBeNull();
    expect(
      defaultValueForSchema({ name: 'user.CycleA', type: 'ref' }, lookup),
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
      defaultValueForSchema(
        { type: 'wormhole' } as unknown as FieldSchema,
        lookup,
      ),
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

  it('matches enum refs by variant membership', () => {
    expect(valueMatchesSchema('Blue', colorRef, lookup)).toBe(true);
    expect(valueMatchesSchema('Magenta', colorRef, lookup)).toBe(false);
    expect(valueMatchesSchema(2, colorRef, lookup)).toBe(false);
  });

  it('lets a plain object inhabit both a class and a map schema', () => {
    const mapSchema: FieldSchema = {
      key: { type: 'string' },
      type: 'map',
      value: { type: 'int' },
    };
    const instance = { name: 'A' };
    expect(valueMatchesSchema(instance, personRef, lookup)).toBe(true);
    expect(valueMatchesSchema(instance, mapSchema, lookup)).toBe(true);
    expect(valueMatchesSchema([1], personRef, lookup)).toBe(false);
    expect(valueMatchesSchema('A', mapSchema, lookup)).toBe(false);
  });

  it('matches recursive aliases and admits anything for dangling refs', () => {
    const jsonRef: FieldSchema = { name: 'user.JSON', type: 'ref' };
    expect(valueMatchesSchema('text', jsonRef, lookup)).toBe(true);
    expect(valueMatchesSchema(['a', ['b']], jsonRef, lookup)).toBe(true);
    expect(valueMatchesSchema(7, jsonRef, lookup)).toBe(false);
    expect(valueMatchesSchema(7, danglingRef, lookup)).toBe(true);
  });

  it('selects the union variant the value inhabits', () => {
    // An enum variant is a string too, so the declared order decides.
    const variants: FieldSchema[] = [
      { type: 'int' },
      colorRef,
      { type: 'string' },
      { item: { type: 'int' }, type: 'list' },
    ];
    expect(activeUnionVariant(3, variants, lookup)).toBe(0);
    expect(activeUnionVariant('Blue', variants, lookup)).toBe(1);
    expect(activeUnionVariant('x', variants, lookup)).toBe(2);
    expect(activeUnionVariant([1], variants, lookup)).toBe(3);
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
    const opt: FieldSchema = { inner: { type: 'int' }, type: 'optional' };
    expect(valueMatchesSchema(null, opt, lookup)).toBe(true);
    expect(valueMatchesSchema(2, opt, lookup)).toBe(true);
    expect(valueMatchesSchema('x', opt, lookup)).toBe(false);
  });
});

describe('schemaLabel', () => {
  it('collapses FQNs and composes container labels', () => {
    expect(schemaLabel(colorRef)).toBe('Color');
    expect(schemaLabel(personRef)).toBe('Person');
    expect(schemaLabel({ item: personRef, type: 'list' })).toBe('Person[]');
    expect(
      schemaLabel({
        name: 'user.Status',
        type: 'enumVariant',
        value: 'Active',
      }),
    ).toBe('Status.Active');
    expect(schemaLabel({ inner: { type: 'string' }, type: 'optional' })).toBe(
      'string?',
    );
    expect(
      schemaLabel({
        type: 'union',
        variants: [{ type: 'int' }, { type: 'string' }],
      }),
    ).toBe('int | string');
    expect(schemaLabel({ kind: 'image', type: 'media' })).toBe('image');
    expect(schemaLabel({ display: 'callback', type: 'unsupported' })).toBe(
      'callback',
    );
  });
});

describe('isRawJsonSchema', () => {
  it('sends unsupported, media, dangling refs, and unknown tags to raw JSON', () => {
    expect(isRawJsonSchema({ display: 'T', type: 'unsupported' }, lookup)).toBe(
      true,
    );
    expect(isRawJsonSchema({ kind: 'pdf', type: 'media' }, lookup)).toBe(true);
    expect(isRawJsonSchema(danglingRef, lookup)).toBe(true);
    expect(isRawJsonSchema({ name: 'user.CycleA', type: 'ref' }, lookup)).toBe(
      true,
    );
    expect(
      isRawJsonSchema({ type: 'wormhole' } as unknown as FieldSchema, lookup),
    ).toBe(true);
    expect(isRawJsonSchema(personRef, lookup)).toBe(false);
    expect(isRawJsonSchema(colorRef, lookup)).toBe(false);
    expect(isRawJsonSchema({ name: 'user.JSON', type: 'ref' }, lookup)).toBe(
      false,
    );
    expect(isRawJsonSchema({ type: 'string' }, lookup)).toBe(false);
  });
});

describe('reconcileArgs', () => {
  const params: ParamSchema[] = [
    { hasDefault: false, name: 'c', schema: colorRef },
    { hasDefault: false, name: 'p', schema: personRef },
    {
      hasDefault: false,
      name: 'list',
      schema: { item: colorRef, type: 'list' },
    },
  ];

  it('keeps enum names that name a variant and resets the rest', () => {
    const valid = { c: 'Red' };
    expect(reconcileArgs(valid, [params[0]], lookup)).toBe(valid);
    const list = { list: ['Red', 'Blue'] };
    expect(reconcileArgs(list, [params[2]], lookup)).toBe(list);
    // An incompatible stale value resets to the schema default.
    expect(reconcileArgs({ c: 'Magenta' }, [params[0]], lookup)).toEqual({
      c: 'Red',
    });
  });

  it('fills missing class fields with defaults', () => {
    expect(
      reconcileArgs(
        { p: { color: 'Green', name: 'Ada' } },
        [params[1]],
        lookup,
      ),
    ).toEqual({
      p: {
        age: null,
        color: 'Green',
        name: 'Ada',
        nested: { x: 0 },
      },
    });
  });

  it('fills partial class objects and is idempotent once complete', () => {
    const args = { p: { color: 'Blue' } };
    expect(reconcileArgs(args, [params[1]], lookup)).toEqual({
      p: {
        age: null,
        color: 'Blue',
        name: '',
        nested: { x: 0 },
      },
    });
    // Idempotent: complete input comes back by reference.
    const complete = {
      p: {
        age: null,
        color: 'Red',
        name: 'Ada',
        nested: { x: 0 },
      },
    };
    expect(reconcileArgs(complete, [params[1]], lookup)).toBe(complete);
  });

  it('preserves surplus keys and values without schemas', () => {
    const args = { c: 'Red', extra: { a: 1 } };
    expect(reconcileArgs(args, [params[0]], lookup)).toBe(args);
  });

  it('adds required params while leaving declared defaults omitted', () => {
    const requiredAndDefaulted: ParamSchema[] = [
      { hasDefault: false, name: 'required', schema: { type: 'int' } },
      { hasDefault: true, name: 'declared', schema: { type: 'bool' } },
    ];
    expect(reconcileArgs({}, requiredAndDefaulted, lookup)).toEqual({
      required: 0,
    });
  });

  it('migrates compatible values when a class gains a required field', () => {
    const migratedLookup = typeLookupFrom({
      'user.Person': {
        fields: [
          { name: 'name', schema: { type: 'string' } },
          { name: 'active', schema: { type: 'bool' } },
          { name: 'age', schema: { type: 'int' } },
        ],
        kind: 'class',
      },
    });
    expect(
      reconcileArgs(
        {
          p: {
            active: false,
            name: 'Ada',
          },
        },
        [{ hasDefault: false, name: 'p', schema: personRef }],
        migratedLookup,
      ),
    ).toEqual({
      p: {
        active: false,
        age: 0,
        name: 'Ada',
      },
    });
  });

  it('resets a value that no longer matches the parameter type', () => {
    expect(
      reconcileArgs(
        { p: { name: 'Ada' } },
        [{ hasDefault: false, name: 'p', schema: { type: 'int' } }],
        lookup,
      ),
    ).toEqual({ p: 0 });
  });

  it('does not grow a required recursive cut point on repeated reconciliation', () => {
    const node = defaultValueForSchema(
      { name: 'user.RequiredNode', type: 'ref' },
      lookup,
    );
    const args = { node };
    expect(
      reconcileArgs(
        args,
        [
          {
            hasDefault: false,
            name: 'node',
            schema: { name: 'user.RequiredNode', type: 'ref' },
          },
        ],
        lookup,
      ),
    ).toBe(args);
  });
});
