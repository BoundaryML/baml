import { BamlRuntime, callFunction, callFunctionSync } from '../dist/index.js';
import { BamlTypeMap, getTypeMap, setTypeMap } from '../dist/typemap.js';

class Item {
    constructor(init: Record<string, unknown>) { Object.assign(this, init); }
}

class Box {
    static readonly $generic = ['T'];
    constructor(init: Record<string, unknown>) { Object.assign(this, init); }
}

class ItemBox {
    constructor(init: Record<string, unknown>) { Object.assign(this, init); }
}

// Explicit generic bindings isolate annotation preservation from erased-type inference.
const cases = [
    { name: 'Ints', type: 'int[]', read: 'value[0]', make: () => [7] },
    { name: 'Empty', type: 'Item[]', read: 'value.length()', make: () => [], expected: 0 },
    { name: 'Items', type: 'Item[]', read: 'value[0].value', make: () => [new Item({ value: 7 })] },
    { name: 'ItemMap', type: 'map<string, Item>', read: 'value["a"].value', make: () => ({ a: new Item({ value: 7 }) }) },
    { name: 'Boxed', type: 'ItemBox', read: 'value.items[0].value', make: () => new ItemBox({ items: [new Item({ value: 7 })] }) },
    { name: 'GenericBoxed', type: 'Box<Item[]>', read: 'value.value[0].value', make: () => new Box({ value: [new Item({ value: 7 })], $types: { T: { list: Item } } }) },
    { name: 'GenericMapBoxed', type: 'Box<map<string, Item>>', read: 'value.value["a"].value', make: () => new Box({ value: { a: new Item({ value: 7 }) }, $types: { T: { map: ['string', Item] } } }) },
    { name: 'GenericItems', type: 'Box<int>[]', read: 'value[0].value', make: () => [new Box({ value: 7, $types: { T: 'int' } })] },
    { name: 'GenericItemMap', type: 'map<string, Box<int>>', read: 'value["a"].value', make: () => ({ a: new Box({ value: 7, $types: { T: 'int' } }) }) },
    { name: 'Nested', type: 'map<string, Item[]>[]', read: 'value[0]["a"][0].value', make: () => [{ a: [new Item({ value: 7 })] }] },
    { name: 'AliasItems', type: 'Recursive[]', read: 'value.length()', make: () => [new Item({ value: 7 })], expected: 1 },
    { name: 'AliasItemMap', type: 'map<string, Recursive>', read: 'value.length()', make: () => ({ a: new Item({ value: 7 }) }), expected: 1 },
];

const source = `
class Item { value int }
class Box<T> { value T }
class ItemBox { items Item[] }
type Recursive = Item | Recursive[]
${cases.map(({ name, type, read }) => `
function Arg${name}(value: ${type}) -> int { ${read} }
function Host${name}(f: () -> ${type}) -> int { let value = f(); ${read} }
`).join('\n')}
function AsArgument(xs: Item[], f: (Item[]) -> int) -> int { f(xs) }
`;

describe('host callable class containers (#5226)', () => {
    let runtime: BamlRuntime;
    let previous: BamlTypeMap;

    beforeAll(() => {
        previous = getTypeMap();
        runtime = BamlRuntime.initializeRuntime('.', { 'main.baml': source });
        setTypeMap(BamlTypeMap.fromLazyEntries({
            classes: { 'user.Item': () => Item, 'user.Box': () => Box, 'user.ItemBox': () => ItemBox },
            enums: {},
            typeAliases: {},
        }));
    });
    afterAll(() => setTypeMap(previous));

    test.each(cases)('$type ($name) resolves identically for arguments and callback returns', async ({ name, make, expected = 7 }) => {
        expect((await callFunction(runtime, `Arg${name}`, { value: make() })).result()).toBe(expected);
        expect(callFunctionSync(runtime, `Arg${name}`, { value: make() }).result()).toBe(expected);
        for (const f of [() => make(), async () => make()]) {
            expect((await callFunction(runtime, `Host${name}`, { f })).result()).toBe(expected);
        }
    });

    test('a class array passed to a callback still works', async () => {
        expect((await callFunction(runtime, 'AsArgument', {
            xs: [new Item({ value: 7 })], f: async (xs: Item[]) => xs.length,
        })).result()).toBe(1);
    });

    test.each([
        ['Items', () => [new Item({ value: 'bad' })]],
        ['ItemMap', () => ({ a: new Item({ value: 'bad' }) })],
        ['GenericItems', () => [new Box({ value: 'bad', $types: { T: 'int' } })]],
        ['GenericItemMap', () => ({ a: new Box({ value: 'bad', $types: { T: 'int' } }) })],
        ['GenericItems', () => [new Box({ value: 'bad', $types: { T: 'string' } })]],
        ['GenericItemMap', () => ({ a: new Box({ value: 'bad', $types: { T: 'string' } }) })],
        ['AliasItems', () => [new Item({ value: 'bad' })]],
        ['AliasItemMap', () => ({ a: new Item({ value: 'bad' }) })],
    ] as const)('rejects invalid class fields or generic arguments in %s (%#)', async (name, make) => {
        await expect(callFunction(runtime, `Arg${name}`, { value: make() })).rejects.toThrow();
        await expect(callFunction(runtime, `Host${name}`, { f: async () => make() })).rejects.toThrow(/HostContractViolation/);
    });
});
