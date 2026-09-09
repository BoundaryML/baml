import { expect, test } from 'vitest';
import { BamlType, Never, type BamlTypeValue } from '../dist/wire_ty.js';
import { BamlInterfaceRef, BamlInterfaceType } from '../dist/interface_ref.js';

class HarnessRef extends BamlInterfaceRef {}
const target = (a: BamlTypeValue, b: BamlTypeValue) =>
    BamlInterfaceType._create(HarnessRef, 'user.Source', [], [['Output', a], ['Error', b]]);

test('JavaScript callers cannot construct type evidence from an unchecked schema', () => {
    expect(() => Reflect.construct(BamlType, [{ root: { never: {} } }])).toThrow(/SDK factories/);
});

test('type composition copies definition graphs without changing its inputs', () => {
    const value = BamlType.from('string');
    const composed = value.array().optional();
    const exported = composed._wireCopy();
    exported.root = null;
    expect(value._wireCopy().root?.primitive).toBeDefined();
    expect(composed._wireCopy().root?.optional?.inner?.list?.item?.primitive).toBeDefined();
});

test('embedding root-scoped conformance fails before changing its owner', () => {
    const reflected = BamlType._fromWire({
        root: { classTy: { name: 'Dynamic' } },
        classes: [{ name: 'Dynamic', fields: [] }],
        witnesses: [{ interface: 'user.Marker' }],
    });
    expect(() => reflected.array()).toThrow(/root-scoped/);
    expect(() => reflected.optional()).toThrow(/root-scoped/);
    expect(() => target(reflected, BamlType.from(Never))).toThrow(/root-scoped/);
    expect(reflected._wireCopy().witnesses).toHaveLength(1);
});

test('conflicting nominal definitions cannot be hidden by composing type arguments', () => {
    const value = (field: string) => BamlType._fromWire({
        root: { classTy: { name: 'Dynamic' } },
        classes: [{ name: 'Dynamic', fields: [{ name: field, ty: { never: {} } }] }],
    });
    expect(() => target(value('one'), value('two'))).toThrow('conflicting type definitions for Dynamic');
    const compatible = target(value('same'), value('same'))._wireCopy();
    expect(compatible.classes).toHaveLength(1);
    expect(compatible.root?.interface?.bindings).toHaveLength(2);
});

test('erased wire boundaries accept exact never evidence without changing the token', () => {
    const bottom = BamlType.from(Never);
    const descriptor = BamlInterfaceType._create(HarnessRef, 'user.Source', [], [['Error', bottom]]);
    expect(descriptor._wireCopy().root?.interface?.bindings?.[0].ty?.never).toBeDefined();
});

test('declared type evidence keeps its issuing SDK after global replacement', async () => {
    const { BamlTypeMap, getTypeMap, setTypeMap } = await import('../dist/typemap.js');
    class Box {}
    const issuer = BamlTypeMap.fromLazyEntries({ classes: { 'user.Box': () => Box }, enums: {}, typeAliases: {} });
    // Even an identical constructor in a different SDK must not supply its metadata.
    const other = BamlTypeMap.fromLazyEntries({ classes: { 'user.Box': () => Box }, enums: {}, typeAliases: {} });
    const previous = getTypeMap();
    try {
        setTypeMap(other);
        const text = BamlType.from('string');
        const token = BamlType._declared<Box>(issuer, 'class', 'user.Box', [text]);
        expect(() => token._checkSdk(issuer)).not.toThrow();
        expect(() => token._checkSdk(other)).toThrow(/does not belong/);
        expect(() => token.array()._checkSdk(other)).toThrow(/does not belong/);
        expect(token._wireCopy().root?.classTy?.typeArgs).toHaveLength(1);
        expect(() => BamlType._declared(issuer, 'class', 'user.Missing', [])).toThrow(/Unknown class/);
        const foreign = BamlType._declared(other, 'class', 'user.Box', []);
        expect(() => BamlType._declared(issuer, 'class', 'user.Box', [foreign])).toThrow(/does not belong/);
    } finally { setTypeMap(previous); }
});

test('nominal type factories preserve nested graphs and reject misplaced witnesses', async () => {
    const { BamlTypeMap } = await import('../dist/typemap.js');
    class Box {}
    const issuer = BamlTypeMap.fromLazyEntries({ classes: { 'user.Box': () => Box }, enums: {}, typeAliases: {} });
    const child = BamlType._fromWire({
        root: { classTy: { name: 'Dynamic' } },
        classes: [{ name: 'Dynamic', fields: [{ name: 'color', ty: { enum: { name: 'Color' } } }] }],
        enums: [{ name: 'Color', variants: [{ name: 'Red' }] }],
    });
    const token = BamlType._declared(issuer, 'class', 'user.Box', [child]);
    const wire = token._wireCopy();
    expect(wire.classes).toHaveLength(1);
    expect(wire.enums).toHaveLength(1);
    expect(wire.root?.classTy?.typeArgs?.[0].classTy?.name).toBe('Dynamic');
    wire.classes.length = 0;
    expect(token._wireCopy().classes).toHaveLength(1);
    const withWitness = BamlType._fromWire({ ...child._wireCopy(), witnesses: [{ interface: 'user.Marker' }] });
    expect(() => BamlType._declared(issuer, 'class', 'user.Box', [withWitness])).toThrow(/root-scoped/);
});
