import { describe, expect, it } from 'vitest';

import {
  activeUnionVariant,
  defaultValueForSchema,
  enumValue,
  enumVariantOf,
  isEnumMarkerValue,
  isRawJsonSchema,
  schemaLabel,
  valueMatchesSchema,
} from './args-form-model';
import type { FieldSchema } from './worker-protocol';

const colorEnum: FieldSchema = {
  type: 'enum',
  name: 'user.Color',
  values: ['Red', 'Green', 'Blue'],
};

const personClass: FieldSchema = {
  type: 'class',
  name: 'user.Person',
  recursive: false,
  fields: [
    { name: 'name', schema: { type: 'string' } },
    { name: 'age', schema: { type: 'optional', inner: { type: 'int' } } },
    { name: 'color', schema: colorEnum },
  ],
};

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

describe('defaultValueForSchema', () => {
  it('seeds primitives with zero values', () => {
    expect(defaultValueForSchema({ type: 'string' })).toBe('');
    expect(defaultValueForSchema({ type: 'int' })).toBe(0);
    expect(defaultValueForSchema({ type: 'bool' })).toBe(false);
    expect(defaultValueForSchema({ type: 'optional', inner: { type: 'int' } })).toBeNull();
    expect(defaultValueForSchema({ type: 'list', item: { type: 'int' } })).toEqual([]);
    expect(
      defaultValueForSchema({
        type: 'map',
        key: { type: 'string' },
        value: { type: 'int' },
      }),
    ).toEqual({});
  });

  it('seeds a class with its $baml marker and recursive field defaults', () => {
    expect(defaultValueForSchema(personClass)).toEqual({
      $baml: { type: 'user.Person' },
      name: '',
      age: null,
      color: { $baml: { enum: 'user.Color', value: 'Red' } },
    });
  });

  it('seeds enums with their first variant as a marker', () => {
    expect(defaultValueForSchema(colorEnum)).toEqual(
      enumValue('user.Color', 'Red'),
    );
  });

  it('seeds literals with the literal value and cuts recursion with null', () => {
    expect(defaultValueForSchema({ type: 'literal', value: 42 })).toBe(42);
    expect(
      defaultValueForSchema({
        type: 'class',
        name: 'user.Tree',
        fields: [],
        recursive: true,
      }),
    ).toBeNull();
  });

  it('seeds unions with the first variant', () => {
    expect(
      defaultValueForSchema({
        type: 'union',
        variants: [{ type: 'int' }, { type: 'string' }],
      }),
    ).toBe(0);
  });
});

describe('valueMatchesSchema / activeUnionVariant', () => {
  it('distinguishes primitives structurally', () => {
    expect(valueMatchesSchema('hi', { type: 'string' })).toBe(true);
    expect(valueMatchesSchema(1.5, { type: 'int' })).toBe(false);
    expect(valueMatchesSchema(1.5, { type: 'float' })).toBe(true);
    expect(valueMatchesSchema(null, { type: 'null' })).toBe(true);
  });

  it('matches enum values by marker name and membership', () => {
    expect(valueMatchesSchema(enumValue('user.Color', 'Red'), colorEnum)).toBe(true);
    expect(valueMatchesSchema(enumValue('user.Other', 'Red'), colorEnum)).toBe(false);
    expect(valueMatchesSchema('Blue', colorEnum)).toBe(true);
    expect(valueMatchesSchema('Magenta', colorEnum)).toBe(false);
  });

  it('separates class values from maps by the $baml marker', () => {
    const mapSchema: FieldSchema = {
      type: 'map',
      key: { type: 'string' },
      value: { type: 'int' },
    };
    const instance = { $baml: { type: 'user.Person' }, name: 'A' };
    expect(valueMatchesSchema(instance, personClass)).toBe(true);
    expect(valueMatchesSchema(instance, mapSchema)).toBe(false);
    expect(valueMatchesSchema({ a: 1 }, mapSchema)).toBe(true);
    // A markerless object could be either; class accepts it.
    expect(valueMatchesSchema({ a: 1 }, personClass)).toBe(true);
    expect(
      valueMatchesSchema({ $baml: { type: 'user.Other' } }, personClass),
    ).toBe(false);
  });

  it('selects the union variant the value inhabits', () => {
    const variants: FieldSchema[] = [
      { type: 'int' },
      { type: 'string' },
      { type: 'list', item: { type: 'int' } },
      colorEnum,
    ];
    expect(activeUnionVariant(3, variants)).toBe(0);
    expect(activeUnionVariant('x', variants)).toBe(1);
    expect(activeUnionVariant([1], variants)).toBe(2);
    expect(activeUnionVariant(enumValue('user.Color', 'Blue'), variants)).toBe(3);
    expect(activeUnionVariant(true, variants)).toBe(-1);
  });

  it('treats optional as null-or-inner', () => {
    const opt: FieldSchema = { type: 'optional', inner: { type: 'int' } };
    expect(valueMatchesSchema(null, opt)).toBe(true);
    expect(valueMatchesSchema(2, opt)).toBe(true);
    expect(valueMatchesSchema('x', opt)).toBe(false);
  });
});

describe('schemaLabel', () => {
  it('collapses FQNs and composes container labels', () => {
    expect(schemaLabel(colorEnum)).toBe('Color');
    expect(schemaLabel(personClass)).toBe('Person');
    expect(schemaLabel({ type: 'list', item: personClass })).toBe('Person[]');
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
  it('sends unsupported, media, and recursion cut-points to raw JSON', () => {
    expect(isRawJsonSchema({ type: 'unsupported', display: 'T' })).toBe(true);
    expect(isRawJsonSchema({ type: 'media', kind: 'pdf' })).toBe(true);
    expect(
      isRawJsonSchema({
        type: 'class',
        name: 'user.Tree',
        fields: [],
        recursive: true,
      }),
    ).toBe(true);
    expect(isRawJsonSchema(personClass)).toBe(false);
    expect(isRawJsonSchema({ type: 'string' })).toBe(false);
  });
});
