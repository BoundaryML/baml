// BAMLGH-102 / #5226: callback results must use the same inbound contract as arguments.
import { BamlRuntime } from '../dist/native.js';
import { callFunction, callFunctionSync } from '../dist/index.js';
import { BamlTypeMap, getTypeMap, setTypeMap } from '../dist/typemap.js';

class Item {
    /** Exercise the registered class encoder used by the ticket repro. */
    constructor(init: Record<string, unknown>) { Object.assign(this, init); }
}
class Box {
    /** Exercise the ticket control case with a nested class list. */
    constructor(init: Record<string, unknown>) { Object.assign(this, init); }
}

const source = `
class Item { value: int, }
class Box { items: Item[], }
type ItemList = Item[]
enum Color { Red Green }
enum Mood { Happy Sad }
function Ints(value: int[]) -> int[] { value }
function Items(value: Item[]) -> Item[] { value }
function Boxed(value: Box) -> Item[] { value.items }
function ItemMap(value: map<string, Item>) -> int { value.length() }
function Nested(value: map<string, Item[]>[]) -> int { value[0]["a"][0].value }
function Alias(value: ItemList) -> int { value[0].value }
function Nullable(value: Item[]?) -> Item[]? { value }
function Float(value: float) -> float { value / 2.0 }
function Enum(value: Color) -> Color { value }
function EnumUnion(value: Color | Mood) -> Color | Mood { value }
function Singleton(value: Color.Red) -> Color.Red { value }
function Never(value: never) -> never { value }
function NeverList(value: never[]) -> never[] { value }
function HostInts(f: () -> int[]) -> int[] { f() }
function HostItems(f: () -> Item[]) -> Item[] { f() }
function HostBoxed(f: () -> Box) -> Item[] { f().items }
function HostItemMap(f: () -> map<string, Item>) -> int { f().length() }
function HostNested(f: () -> map<string, Item[]>[]) -> int { f()[0]["a"][0].value }
function HostAlias(f: () -> ItemList) -> int { f()[0].value }
function HostNullable(f: () -> Item[]?) -> Item[]? { f() }
function HostFloat(f: () -> float) -> float { f() / 2.0 }
function HostEnum(f: () -> Color) -> Color { f() }
function HostEnumUnion(f: () -> (Color | Mood)) -> Color | Mood { f() }
function HostSingleton(f: () -> Color.Red) -> Color.Red { f() }
function HostNever(f: () -> never) -> never { f() }
function HostNeverList(f: () -> never[]) -> never[] { f() }
function AsArgument(xs: Item[], f: (Item[]) -> int) -> int { f(xs) }
`;

describe('host return / inbound argument parity', () => {
    let runtime: BamlRuntime;
    let previous: BamlTypeMap;
    beforeAll(() => {
        previous = getTypeMap();
        runtime = BamlRuntime.initializeRuntime('.', { 'main.baml': source });
        setTypeMap(BamlTypeMap.fromLazyEntries({
            classes: { 'user.Item': () => Item, 'user.Box': () => Box },
            enums: { 'user.Color': () => ({ Red: 'Red', Green: 'Green' }), 'user.Mood': () => ({ Happy: 'Happy', Sad: 'Sad' }) },
            typeAliases: {},
        }));
    });
    afterAll(() => setTypeMap(previous));

    /** Build a fresh registered value for each ingress path. */
    const item = () => new Item({ value: 7 });
    const cases: [string, string, () => unknown, unknown][] = [
        ['primitive list', 'Ints', () => [1, 2], [1, 2]],
        ['empty class list', 'Items', () => [], []],
        ['non-empty class list (ticket repro)', 'Items', () => [item()], [item()]],
        ['boxed class list', 'Boxed', () => new Box({ items: [item()] }), [item()]],
        ['non-empty class map (ticket repro)', 'ItemMap', () => ({ a: item() }), 1],
        ['nested containers', 'Nested', () => [{ a: [item()] }], 7],
        ['alias', 'Alias', () => [item()], 7],
        ['nullable class list', 'Nullable', () => [item()], [item()]],
        ['null', 'Nullable', () => null, null],
        ['structural class payload', 'Items', () => [{ value: 7 }], [item()]],
        ['contextual JavaScript number', 'Float', () => 612, 306],
        ['string enum coercion', 'Enum', () => 'Red', 'Red'],
        ['singleton enum string coercion', 'Singleton', () => 'Red', 'Red'],
        ['unique enum string union member', 'EnumUnion', () => 'Sad', 'Sad'],
        ['first enum string union member', 'EnumUnion', () => 'Red', 'Red'],
        ['empty never list', 'NeverList', () => [], []],
    ];

    test.each(cases)('%s', async (_label, name, value, expected) => {
        expect((await callFunction(runtime, name, { value: value() })).result()).toStrictEqual(expected);
        expect(callFunctionSync(runtime, name, { value: value() }).result()).toStrictEqual(expected);
        for (const f of [() => value(), async () => value()]) {
            expect((await callFunction(runtime, `Host${name}`, { f })).result()).toStrictEqual(expected);
        }
    });

    test('the ticket argument-to-callback control', async () => {
        expect((await callFunction(runtime, 'AsArgument', {
            xs: [item()], f: async (xs: Item[]) => xs.length,
        })).result()).toBe(1);
    });

    const invalid: [string, string, () => unknown, RegExp][] = [
        ['wrong root kind', 'Items', () => 42, /int|number/],
        ['wrong list element', 'Items', () => [42], /int|number/],
        ['wrong map value', 'ItemMap', () => ({ a: 42 }), /int|number/],
        ['wrong nested field', 'Nested', () => [{ a: [new Item({ value: 'bad' })] }], /string/],
        ['wrong boxed field', 'Boxed', () => new Box({ items: [new Item({ value: 'bad' })] }), /string/],
        ['missing field', 'Items', () => [new Item({})], /Missing field.*value/],
        ['wrong nullable payload', 'Nullable', () => ['bad'], /array.*union/],
        ['wrong numeric value', 'Float', () => 'bad', /string/],
        ['unknown enum member', 'Enum', () => 'Missing', /Missing/],
        ['wrong singleton enum string', 'Singleton', () => 'Green', /Green|Red/],
        ['unknown enum union member', 'EnumUnion', () => 'Missing', /Missing|union/],
        ['never value', 'Never', () => 1, /never/],
        ['nonempty never list', 'NeverList', () => [1], /never/],
    ];
    test.each(invalid)('rejects %s', async (_label, name, value, detail) => {
        await expect(callFunction(runtime, name, { value: value() })).rejects.toThrow(detail);
        expect(() => callFunctionSync(runtime, name, { value: value() })).toThrow(detail);
        for (const f of [() => value(), async () => value()]) {
            const error = await callFunction(runtime, `Host${name}`, { f }).catch(e => e);
            expect(error).toBeInstanceOf(Error);
            expect(error.message).toMatch(/HostContractViolation/);
            expect(error.message).toMatch(detail);
        }
    });
});
