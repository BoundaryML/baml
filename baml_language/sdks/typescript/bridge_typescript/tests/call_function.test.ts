/**
 * Tests for bridge_typescript: callFunctionSync and callFunction (async) covering
 * classes, enums, unions, lists, maps, nested classes, and error cases.
 *
 * All BAML source is embedded inline. No LLM calls — only pure expression
 * functions — so these run without API keys.
 */

import { BamlRuntime } from '../dist/native.js';
import { callFunctionSync, callFunction, FunctionResult } from '../dist/index.js';

// ============================================================================
// BAML source used by tests
// ============================================================================

const BAML_SOURCE = `
// ── Simple scalars (keep two for smoke testing) ──

function ReturnOne() -> int {
    1
}

function Identity(s: string) -> string {
    s
}

// ── Enums ──

enum Color {
    Red,
    Green,
    Blue,
}

function PickColor(x: int) -> Color {
    if (x > 0) { Color.Red } else { Color.Blue }
}

enum Status {
    Active,
    Inactive,
    Pending,
}

function GetStatus(active: bool) -> Status {
    if (active) { Status.Active } else { Status.Inactive }
}

// ── Classes ──

class Point {
    x int
    y int
}

function MakePoint(x: int, y: int) -> Point {
    Point { x: x, y: y }
}

class Person {
    name string
    age int
    active bool
}

function MakePerson(n: string, a: int) -> Person {
    Person { name: n, age: a, active: true }
}

// ── Nested classes ──

class Address {
    city string
    zip int
}

class Contact {
    name string
    address Address
}

function MakeContact(name: string, city: string, zip: int) -> Contact {
    Contact { name: name, address: Address { city: city, zip: zip } }
}

// ── Unions ──

function IntOrString(asInt: bool, x: int) -> int | string {
    if (asInt) { x } else { "hello" }
}

function ColorOrPoint(useColor: bool) -> Color | Point {
    if (useColor) { Color.Green } else { Point { x: 1, y: 2 } }
}

function ClassifyAmbiguousEmptyList(value: int[] | string[]) -> string {
    match (value) {
        let ints: int[] => "ints",
        let strings: string[] => "strings",
    }
}

// ── Lists and maps ──

function MakeIntList(a: int, b: int, c: int) -> int[] {
    [a, b, c]
}

function MakeStringMap() -> map<string, int> {
    {"a": 1, "b": 2, "c": 3}
}

// ── Class with list field ──

class Tagged {
    label string
    scores int[]
}

function MakeTagged(label: string) -> Tagged {
    Tagged { label: label, scores: [10, 20, 30] }
}

// ── Bigint round-trip ──

function EchoBigint(x: bigint) -> bigint {
    x
}
`;

// ============================================================================
// Helper
// ============================================================================

function makeRuntime(bamlSource: string): BamlRuntime {
    return BamlRuntime.initializeRuntime('.', { 'main.baml': bamlSource });
}

// ============================================================================
// Tests
// ============================================================================

describe('callFunctionSync', () => {
    let rt: BamlRuntime;

    beforeAll(() => {
        rt = makeRuntime(BAML_SOURCE);
    });

    // ── Smoke tests (simple scalars) ──

    it('ReturnOne → 1', () => {
        const result = callFunctionSync(rt, 'ReturnOne', {});
        expect(result).toBeInstanceOf(FunctionResult);
        expect(result.result()).toBe(1);
    });

    it('Identity("hello") → "hello"', () => {
        const result = callFunctionSync(rt, 'Identity', { s: 'hello' });
        expect(result.result()).toBe('hello');
    });

    // ── Enum results ──

    it('PickColor(1) → "Red"', () => {
        const result = callFunctionSync(rt, 'PickColor', { x: 1 });
        expect(result.result()).toBe('Red');
    });

    it('PickColor(-1) → "Blue"', () => {
        const result = callFunctionSync(rt, 'PickColor', { x: -1 });
        expect(result.result()).toBe('Blue');
    });

    it('GetStatus(true) → "Active"', () => {
        const result = callFunctionSync(rt, 'GetStatus', { active: true });
        expect(result.result()).toBe('Active');
    });

    it('GetStatus(false) → "Inactive"', () => {
        const result = callFunctionSync(rt, 'GetStatus', { active: false });
        expect(result.result()).toBe('Inactive');
    });

    // ── Class results ──

    it('MakePoint(3, 4) → { x: 3, y: 4 }', () => {
        const result = callFunctionSync(rt, 'MakePoint', { x: 3, y: 4 });
        expect(result.result()).toEqual({ x: 3, y: 4 });
    });

    it('MakePerson("Alice", 30) → { name, age, active }', () => {
        const result = callFunctionSync(rt, 'MakePerson', { n: 'Alice', a: 30 });
        expect(result.result()).toEqual({
            name: 'Alice',
            age: 30,
            active: true,
        });
    });

    // ── Nested class results ──

    it('MakeContact returns nested class', () => {
        const result = callFunctionSync(rt, 'MakeContact', {
            name: 'Bob',
            city: 'Seattle',
            zip: 98101,
        });
        expect(result.result()).toEqual({
            name: 'Bob',
            address: { city: 'Seattle', zip: 98101 },
        });
    });

    // ── Union results ──

    it('IntOrString(true, 42) → int branch', () => {
        const result = callFunctionSync(rt, 'IntOrString', { asInt: true, x: 42 });
        expect(result.result()).toBe(42);
    });

    it('IntOrString(false, 42) → string branch', () => {
        const result = callFunctionSync(rt, 'IntOrString', { asInt: false, x: 42 });
        expect(result.result()).toBe('hello');
    });

    it('ColorOrPoint(true) → enum branch (Color.Green)', () => {
        const result = callFunctionSync(rt, 'ColorOrPoint', { useColor: true });
        expect(result.result()).toBe('Green');
    });

    it('ColorOrPoint(false) → class branch (Point)', () => {
        const result = callFunctionSync(rt, 'ColorOrPoint', { useColor: false });
        expect(result.result()).toEqual({ x: 1, y: 2 });
    });

    it('raw [] uses the dynamic default for int[] | string[]', () => {
        const result = callFunctionSync(rt, 'ClassifyAmbiguousEmptyList', { value: [] });
        expect(result.result()).toBe('ints');
    });

    // ── Lists and maps ──

    it('MakeIntList(1, 2, 3) → [1, 2, 3]', () => {
        const result = callFunctionSync(rt, 'MakeIntList', { a: 1, b: 2, c: 3 });
        expect(result.result()).toEqual([1, 2, 3]);
    });

    it('MakeStringMap() → { a: 1, b: 2, c: 3 }', () => {
        const result = callFunctionSync(rt, 'MakeStringMap', {});
        expect(result.result()).toEqual({ a: 1, b: 2, c: 3 });
    });

    // ── Class with list field ──

    it('MakeTagged returns class with list field', () => {
        const result = callFunctionSync(rt, 'MakeTagged', { label: 'test' });
        expect(result.result()).toEqual({
            label: 'test',
            scores: [10, 20, 30],
        });
    });

    // ── Bigint round-trip ──

    it('EchoBigint round-trips a small bigint', () => {
        const result = callFunctionSync(rt, 'EchoBigint', { x: 42n });
        const value = result.result();
        expect(typeof value).toBe('bigint');
        expect(value).toBe(42n);
    });

    it('EchoBigint round-trips a large positive bigint', () => {
        const huge = 99999999999999999999n;
        const result = callFunctionSync(rt, 'EchoBigint', { x: huge });
        const value = result.result();
        expect(typeof value).toBe('bigint');
        expect(value).toBe(huge);
    });

    it('EchoBigint round-trips a negative bigint', () => {
        const result = callFunctionSync(rt, 'EchoBigint', { x: -42n });
        const value = result.result();
        expect(typeof value).toBe('bigint');
        expect(value).toBe(-42n);
    });

    // ── Error case ──

    it('throws for unknown function', () => {
        expect(() => callFunctionSync(rt, 'NonExistent', {})).toThrow();
    });
});

describe('callFunction (async)', () => {
    let rt: BamlRuntime;

    beforeAll(() => {
        rt = makeRuntime(BAML_SOURCE);
    });

    // ── Smoke tests (simple scalars) ──

    it('ReturnOne → 1', async () => {
        const result = await callFunction(rt, 'ReturnOne', {});
        expect(result).toBeInstanceOf(FunctionResult);
        expect(result.result()).toBe(1);
    });

    it('Identity("hello") → "hello"', async () => {
        const result = await callFunction(rt, 'Identity', { s: 'hello' });
        expect(result.result()).toBe('hello');
    });

    // ── Enum results ──

    it('PickColor(1) → "Red"', async () => {
        const result = await callFunction(rt, 'PickColor', { x: 1 });
        expect(result.result()).toBe('Red');
    });

    it('PickColor(-1) → "Blue"', async () => {
        const result = await callFunction(rt, 'PickColor', { x: -1 });
        expect(result.result()).toBe('Blue');
    });

    it('GetStatus(true) → "Active"', async () => {
        const result = await callFunction(rt, 'GetStatus', { active: true });
        expect(result.result()).toBe('Active');
    });

    it('GetStatus(false) → "Inactive"', async () => {
        const result = await callFunction(rt, 'GetStatus', { active: false });
        expect(result.result()).toBe('Inactive');
    });

    // ── Class results ──

    it('MakePoint(3, 4) → { x: 3, y: 4 }', async () => {
        const result = await callFunction(rt, 'MakePoint', { x: 3, y: 4 });
        expect(result.result()).toEqual({ x: 3, y: 4 });
    });

    it('MakePerson("Alice", 30) → { name, age, active }', async () => {
        const result = await callFunction(rt, 'MakePerson', { n: 'Alice', a: 30 });
        expect(result.result()).toEqual({
            name: 'Alice',
            age: 30,
            active: true,
        });
    });

    // ── Nested class results ──

    it('MakeContact returns nested class', async () => {
        const result = await callFunction(rt, 'MakeContact', {
            name: 'Bob',
            city: 'Seattle',
            zip: 98101,
        });
        expect(result.result()).toEqual({
            name: 'Bob',
            address: { city: 'Seattle', zip: 98101 },
        });
    });

    // ── Union results ──

    it('IntOrString(true, 42) → int branch', async () => {
        const result = await callFunction(rt, 'IntOrString', { asInt: true, x: 42 });
        expect(result.result()).toBe(42);
    });

    it('IntOrString(false, 42) → string branch', async () => {
        const result = await callFunction(rt, 'IntOrString', { asInt: false, x: 42 });
        expect(result.result()).toBe('hello');
    });

    it('ColorOrPoint(true) → enum branch (Color.Green)', async () => {
        const result = await callFunction(rt, 'ColorOrPoint', { useColor: true });
        expect(result.result()).toBe('Green');
    });

    it('ColorOrPoint(false) → class branch (Point)', async () => {
        const result = await callFunction(rt, 'ColorOrPoint', { useColor: false });
        expect(result.result()).toEqual({ x: 1, y: 2 });
    });

    // ── Lists and maps ──

    it('MakeIntList(1, 2, 3) → [1, 2, 3]', async () => {
        const result = await callFunction(rt, 'MakeIntList', { a: 1, b: 2, c: 3 });
        expect(result.result()).toEqual([1, 2, 3]);
    });

    it('MakeStringMap() → { a: 1, b: 2, c: 3 }', async () => {
        const result = await callFunction(rt, 'MakeStringMap', {});
        expect(result.result()).toEqual({ a: 1, b: 2, c: 3 });
    });

    // ── Class with list field ──

    it('MakeTagged returns class with list field', async () => {
        const result = await callFunction(rt, 'MakeTagged', { label: 'test' });
        expect(result.result()).toEqual({
            label: 'test',
            scores: [10, 20, 30],
        });
    });

    // ── Bigint round-trip ──

    it('EchoBigint round-trips a small bigint', async () => {
        const result = await callFunction(rt, 'EchoBigint', { x: 42n });
        const value = result.result();
        expect(typeof value).toBe('bigint');
        expect(value).toBe(42n);
    });

    it('EchoBigint round-trips a large positive bigint', async () => {
        const huge = 99999999999999999999n;
        const result = await callFunction(rt, 'EchoBigint', { x: huge });
        const value = result.result();
        expect(typeof value).toBe('bigint');
        expect(value).toBe(huge);
    });

    it('EchoBigint round-trips a negative bigint', async () => {
        const result = await callFunction(rt, 'EchoBigint', { x: -42n });
        const value = result.result();
        expect(typeof value).toBe('bigint');
        expect(value).toBe(-42n);
    });

    // ── Error case ──

    it('rejects for unknown function', async () => {
        await expect(callFunction(rt, 'NonExistent', {})).rejects.toThrow();
    });
});
