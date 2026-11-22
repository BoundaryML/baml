class C0 { number: int }
class C1 {}

function groupedClass(h: int | C0) -> int | string | C0 {
    let g: (int | string) | C0 {} = h;
    g
}

test "groupedClass covers both members" {
    let c = C0 { number: 42 };
    assert.equal(groupedClass(7), 7);
    let result = groupedClass(c);
    assert.equal(result is C0, true);
    assert.equal(result, c)
}

function flatClass(h: int | C0) -> int | string | C0 {
    let g: int | string | C0 {} = h;
    g
}

test "flatClass covers both members" {
    let c = C0 { number: 42 };
    assert.equal(flatClass(7), 7);
    let result = flatClass(c);
    assert.equal(result is C0, true);
    assert.equal(result, c)
}

function groupedNested(h: int | C0) -> int | string | C0 {
    let g: let k: (int | string) | C0 = h;
    k
}

test "groupedNested covers both members" {
    let c = C0 { number: 42 };
    assert.equal(groupedNested(7), 7);
    let result = groupedNested(c);
    assert.equal(result is C0, true);
    assert.equal(result, c)
}

function flatNested(h: int | C0) -> int | string | C0 {
    let g: let k: int | string | C0 = h;
    k
}

test "flatNested covers both members" {
    let c = C0 { number: 42 };
    assert.equal(flatNested(7), 7);
    let result = flatNested(c);
    assert.equal(result is C0, true);
    assert.equal(result, c)
}

function matchGrouped(h: C0 | C1) -> int {
    match (h) {
        let g: (int | string) | C0 => 1,
        _ => 2,
    }
}

test "matchGrouped selects the expected arm" {
    assert.equal(matchGrouped(C0 { number: 42 }), 1);
    assert.equal(matchGrouped(C1 {}), 2)
}

function matchFlat(h: C0 | C1) -> int {
    match (h) {
        let g: int | string | C0 => 1,
        _ => 2,
    }
}

test "matchFlat selects the expected arm" {
    assert.equal(matchFlat(C0 { number: 42 }), 1);
    assert.equal(matchFlat(C1 {}), 2)
}

function elseBinding(h: C0 | C1) -> int {
    let g: let k: C0 = h else { return 0; };
    k.number
}

test "let else handles both paths" {
    assert.equal(elseBinding(C0 { number: 42 }), 42);
    assert.equal(elseBinding(C1 {}), 0)
}

test "plain binding infers its value" {
    let x = 7;
    assert.equal(x + 1, 8)
}

test "ascribed rest covers a list of the declared type" {
    let xs: [..]: int[] = [4, 5, 6];
    assert.equal(xs.length(), 3)
}

class Tagged { tag: int }

function constrainedElse(h: int | Tagged) -> int {
    let g: int | Tagged { tag: 1 } = h else { return 0; };
    1
}

test "field constraint uses the else branch" {
    assert.equal(constrainedElse(5), 1);
    assert.equal(constrainedElse(Tagged { tag: 1 }), 1);
    assert.equal(constrainedElse(Tagged { tag: 2 }), 0)
}

class Flag { tag: bool }

function booleanAlternatives(h: int | Flag) -> int {
    let g: int | Flag { tag: true } | Flag { tag: false } = h;
    1
}

test "boolean field alternatives cover every input" {
    assert.equal(booleanAlternatives(5), 1);
    assert.equal(booleanAlternatives(Flag { tag: true }), 1);
    assert.equal(booleanAlternatives(Flag { tag: false }), 1)
}

type Either = C0 | C1;
type OptionalC0 = C0?;

function namedAlias(h: Either) -> Either {
    let g: C0 {} | C1 {} = h;
    g
}

function namedDirect(h: C0 | C1) -> C0 | C1 {
    let g: C0 {} | C1 {} = h;
    g
}

function optionalAlias(h: OptionalC0) -> OptionalC0 {
    let g: C0 {} | null = h;
    g
}

function optionalDirect(h: C0?) -> C0? {
    let g: C0 {} | null = h;
    g
}

test "alias and expanded named bindings preserve every member" {
    let c = C0 { number: 41 };
    let d = C1 {};
    assert.equal(namedAlias(c), namedDirect(c));
    assert.equal(namedAlias(d), namedDirect(d));
    assert.equal(namedAlias(c), c);
    assert.equal(namedAlias(d), d);
    assert.equal(optionalAlias(c), optionalDirect(c));
    assert.equal(optionalAlias(null), optionalDirect(null));
    assert.equal(optionalAlias(c), c);
    assert.equal(optionalAlias(null), null)
}

function aliasContexts(h: Either) -> int {
    let C0 {} | C1 {} = h;
    for (let g: C0 {} | C1 {} in [h]) {
        if let selected: C0 {} = g {
            return selected.number;
        }
    }
    -1
}

function directContexts(h: C0 | C1) -> int {
    let C0 {} | C1 {} = h;
    for (let g: C0 {} | C1 {} in [h]) {
        if let selected: C0 {} = g {
            return selected.number;
        }
    }
    -1
}

function aliasElse(h: Either) -> int {
    let g: C0 {} = h else { return 0; };
    g.number
}

function directElse(h: C0 | C1) -> int {
    let g: C0 {} = h else { return 0; };
    g.number
}

test "shared binding contexts select the same alias and direct paths" {
    let c = C0 { number: 41 };
    let d = C1 {};
    assert.equal(aliasContexts(c), 41);
    assert.equal(aliasContexts(d), -1);
    assert.equal(aliasContexts(c), directContexts(c));
    assert.equal(aliasContexts(d), directContexts(d));
    assert.equal(aliasElse(c), 41);
    assert.equal(aliasElse(d), 0);
    assert.equal(aliasElse(c), directElse(c));
    assert.equal(aliasElse(d), directElse(d))
}

function lengthOrFallback(xs: int[]) -> int {
    let pair: [let first, let second] = xs else { return 0; };
    first + second
}

function tupleValue(h: Either) -> int {
    let pair: (let g: C0 {} | C1 {}, let n) = (h, 5);
    n
}

test "fixed length and tuple bindings preserve values and fallback" {
    assert.equal(lengthOrFallback([2, 5]), 7);
    assert.equal(lengthOrFallback([2]), 0);
    assert.equal(lengthOrFallback([]), 0);
    assert.equal(tupleValue(C1 {}), 5)
}

function matchAlias(h: Either) -> int {
    match (h) {
        let g: C0 {} => g.number,
        _ => 0,
    }
}

function matchDirect(h: C0 | C1) -> int {
    match (h) {
        let g: C0 {} => g.number,
        _ => 0,
    }
}

function boundGeneric<T>(h: T) -> T {
    let g: let k = h;
    k
}

test "alias match narrows the selected arm and open bindings retain their type" {
    let c = C0 { number: 41 };
    let d = C1 {};
    assert.equal(matchAlias(c), 41);
    assert.equal(matchAlias(d), 0);
    assert.equal(matchAlias(c), matchDirect(c));
    assert.equal(matchAlias(d), matchDirect(d));
    assert.equal(boundGeneric(c), c);
    assert.equal(boundGeneric("open"), "open")
}

class Boxed { value: Either }
class Direct { value: C0 | C1 }

function pairOverUnion(h: (int, int) | (string, string)) -> int | string {
    let pair: (let first, let second) = h;
    second
}

function fieldAlias(h: Boxed) -> Either {
    let g: Boxed { value: let v: C0 {} | C1 {} } = h;
    v
}

function fieldDirect(h: Direct) -> Either {
    let g: Direct { value: let v: C0 {} | C1 {} } = h;
    v
}

test "tuple unions and nested field aliases preserve each value" {
    let c = C0 { number: 42 };
    let d = C1 {};
    assert.equal(pairOverUnion((2, 3)), 3);
    assert.equal(pairOverUnion(("a", "b")), "b");
    assert.equal(fieldAlias(Boxed { value: c }), c);
    assert.equal(fieldAlias(Boxed { value: d }), d);
    assert.equal(fieldDirect(Direct { value: c }), c);
    assert.equal(fieldDirect(Direct { value: d }), d)
}

class C2 {}
type Three = C0 | C1 | C2

function fullRoutesAlias(h: Three) -> Three[] {
    let named: C0 {} | C1 {} | C2 {} = h;
    let C0 {} | C1 {} | C2 {} = h;
    let checked: C0 {} | C1 {} | C2 {} = h else { return []; };
    let matched = match (h) {
        let m: C0 {} | C1 {} | C2 {} => m,
    };
    for (let item: C0 {} | C1 {} | C2 {} in [h]) {
        if let selected: C0 {} | C1 {} | C2 {} = h {
            return [named, checked, matched, item, selected];
        }
    }
    []
}

function partialElseAlias(h: Three) -> Three? {
    let selected: C0 {} | C1 {} = h else { return null; };
    selected
}

function partialRoutesAlias(h: Three) -> (Three?)[] {
    let matched = match (h) {
        let m: C0 {} | C1 {} => m,
        _ => null,
    };
    let checked = partialElseAlias(h);
    if let selected: C0 {} | C1 {} = h {
        return [selected, matched, checked];
    }
    [null, matched, checked]
}

function fullRoutesDirect(h: C0 | C1 | C2) -> Three[] {
    let named: C0 {} | C1 {} | C2 {} = h;
    let C0 {} | C1 {} | C2 {} = h;
    let checked: C0 {} | C1 {} | C2 {} = h else { return []; };
    let matched = match (h) {
        let m: C0 {} | C1 {} | C2 {} => m,
    };
    for (let item: C0 {} | C1 {} | C2 {} in [h]) {
        if let selected: C0 {} | C1 {} | C2 {} = h {
            return [named, checked, matched, item, selected];
        }
    }
    []
}

function partialElseDirect(h: C0 | C1 | C2) -> Three? {
    let selected: C0 {} | C1 {} = h else { return null; };
    selected
}

function partialRoutesDirect(h: C0 | C1 | C2) -> (Three?)[] {
    let matched = match (h) {
        let m: C0 {} | C1 {} => m,
        _ => null,
    };
    let checked = partialElseDirect(h);
    if let selected: C0 {} | C1 {} = h {
        return [selected, matched, checked];
    }
    [null, matched, checked]
}

test "OR alternatives select each member or fallback in every binding context" {
    let a = C0 { number: 42 };
    let b = C1 {};
    let c = C2 {};
    assert.equal(fullRoutesAlias(a), [a, a, a, a, a]);
    assert.equal(fullRoutesAlias(b), [b, b, b, b, b]);
    assert.equal(fullRoutesAlias(c), [c, c, c, c, c]);
    assert.equal(partialRoutesAlias(a), [a, a, a]);
    assert.equal(partialRoutesAlias(b), [b, b, b]);
    assert.equal(partialRoutesAlias(c), [null, null, null]);
    assert.equal(fullRoutesDirect(a), [a, a, a, a, a]);
    assert.equal(fullRoutesDirect(b), [b, b, b, b, b]);
    assert.equal(fullRoutesDirect(c), [c, c, c, c, c]);
    assert.equal(partialRoutesDirect(a), [a, a, a]);
    assert.equal(partialRoutesDirect(b), [b, b, b]);
    assert.equal(partialRoutesDirect(c), [null, null, null])
}

function tupleAlternativeWithFallback(h: int | C0) -> int {
    match (h) {
        let g: int | (C0 {}, C0 {}) => 1,
        _ => 2,
    }
}

test "an impossible tuple alternative leaves the class for the fallback" {
    assert.equal(tupleAlternativeWithFallback(7), 1);
    assert.equal(tupleAlternativeWithFallback(C0 { number: 42 }), 2)
}
