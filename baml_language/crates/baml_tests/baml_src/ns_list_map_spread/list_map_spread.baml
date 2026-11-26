// Runtime behavior of the spread operator in list and map literals:
// `[a, ...xs]` and `{ k: v, ...m }`.
//
// A spread builds a fresh container and copies the operand's element
// references (or entries) into it at the spread's position. These tests pin:
// element order, override order for maps, strict left-to-right evaluation with
// each operand evaluated once, the operand left unchanged, no aliasing between
// the result and the operand (the reason a `Dog[]` operand may be spread into
// an `Animal[]`), the shallow copy, empty operands, and throwing operands.
//
// Every case is a no-argument `bool` helper asserted with `assert.is_true`: a
// test-block local is boxed (VM quirk), so nothing is bound in a test block.
// The corpus is one project, so every declaration carries an `Lms` / `lms_`
// prefix.

// ─── Shared declarations ─────────────────────────────────────────────────────

interface LmsAnimal {
    function name(self) -> string throws never;
}

class LmsDog {
    tag: string,

    implements LmsAnimal {
        function name(self) -> string throws never {
            "dog:" + self.tag
        }
    }
}

class LmsCat {
    tag: string,

    implements LmsAnimal {
        function name(self) -> string throws never {
            "cat:" + self.tag
        }
    }
}

class LmsLog {
    events: string[],
}

class LmsBox {
    items: int[],
}

class LmsBoom {
    message: string,
}

enum LmsColor {
    Red,
    Green,
    Blue,
}

type LmsInts = int[];

type LmsNest = LmsNest[];

type LmsDict = map<string, int>;

type LmsJsonList = baml.json.json[];

class LmsHolder {
    // A field typed by an alias whose expansion reaches a recursive alias.
    values: LmsJsonList,
    counts: LmsDict,
}

const LMS_BASE: int[] = [1, 2];
const LMS_EXTENDED: int[] = [0, ...LMS_BASE, 3];
const LMS_DEFAULTS: map<string, int> = { "retries": 3, "timeout": 30 };
const LMS_TUNED: map<string, int> = { ...LMS_DEFAULTS, "timeout": 5 };

/// Records `label` and returns `xs`: an operand with a visible side effect.
function lms_list(log: LmsLog, label: string, xs: int[]) -> int[] {
    log.events.push(label);
    xs
}

function lms_item(log: LmsLog, label: string, x: int) -> int {
    log.events.push(label);
    x
}

function lms_map(log: LmsLog, label: string, m: map<string, int>) -> map<string, int> {
    log.events.push(label);
    m
}

function lms_explode_list(log: LmsLog) -> int[] throws LmsBoom {
    log.events.push("explode");
    throw LmsBoom { message: "list" };
}

function lms_explode_map(log: LmsLog) -> map<string, int> throws LmsBoom {
    log.events.push("explode");
    throw LmsBoom { message: "map" };
}

function lms_diverge() -> never throws LmsBoom {
    throw LmsBoom { message: "never" };
}

function lms_copy<T>(xs: T[]) -> T[] {
    [...xs]
}

function lms_concat<T>(head: T, xs: T[], ys: T[]) -> T[] {
    [head, ...xs, ...ys]
}

function lms_merge<K extends baml.Hash & baml.ops.Equals, V>(
    a: map<K, V>,
    b: map<K, V>,
) -> map<K, V> {
    { ...a, ...b }
}

function lms_with_default(xs: int[] = [...LMS_BASE, 9]) -> int[] {
    xs
}

// ─── Lists: element order ────────────────────────────────────────────────────

function lms_list_first() -> bool {
    let xs = [1, 2];
    [...xs, 3] == [1, 2, 3]
}

function lms_list_middle() -> bool {
    let xs = [1, 2];
    [0, ...xs, 3] == [0, 1, 2, 3]
}

function lms_list_last() -> bool {
    let xs = [1, 2];
    [0, ...xs] == [0, 1, 2]
}

function lms_list_only() -> bool {
    let xs = [1, 2];
    [...xs] == [1, 2] && [...xs] == [1, 2]
}

function lms_list_several() -> bool {
    let xs = [1, 2];
    let ys = [9];
    [...xs, ...ys, 5, ...xs, ...ys] == [1, 2, 9, 5, 1, 2, 9]
}

function lms_list_nested() -> bool {
    let xs = [1, 2];
    let ys = [[7], [8]];
    [...[...xs, 3], ...[...ys, [...xs]][2], ...[]] == [1, 2, 3, 1, 2]
        && [[...xs], ...ys] == [[1, 2], [7], [8]]
}

function lms_list_operand_expressions() -> bool {
    let box = LmsBox { items: [4, 5, 6] };
    let flag = true;
    [
        ...box.items.slice(0, 2),
        ...(if (flag) { [7] } else { box.items }),
        ...lms_copy<int>([8]),
    ] == [4, 5, 7, 8]
}

test "list_spread_first_position" {
    assert.is_true(lms_list_first())
}

test "list_spread_middle_position" {
    assert.is_true(lms_list_middle())
}

test "list_spread_last_position" {
    assert.is_true(lms_list_last())
}

test "list_spread_only_element_and_trailing_comma" {
    assert.is_true(lms_list_only())
}

test "list_spread_several_operands" {
    assert.is_true(lms_list_several())
}

test "list_spread_nested" {
    assert.is_true(lms_list_nested())
}

test "list_spread_operand_is_any_expression" {
    assert.is_true(lms_list_operand_expressions())
}

// ─── Lists: the result is a fresh list ───────────────────────────────────────

function lms_list_operand_unchanged() -> bool {
    let xs = [1, 2];
    let out = [0, ...xs, 3];
    xs == [1, 2] && xs.length() == 2 && out.length() == 4
}

function lms_list_result_does_not_alias_operand() -> bool {
    let xs = [1, 2];
    let copy = [...xs];
    copy.push(3);
    copy[0] = 100;
    xs.push(4);
    xs == [1, 2, 4] && copy == [100, 2, 3]
}

function lms_list_empty_operands() -> bool {
    let none: int[] = [];
    let only_empty: int[] = [...none];
    let between = [1, ...none, 2, ...none];
    let literal_empty: int[] = [...[]];
    only_empty.length() == 0
        && between == [1, 2]
        && literal_empty.length() == 0
        && [...none, ...none].length() == 0
}

function lms_list_shallow_copy() -> bool {
    let inner = [1];
    let outer = [inner];
    let copy = [...outer];
    // The inner list is shared: the copy holds the same element references.
    copy[0].push(2);
    // The outer list is not: replacing a slot of the copy leaves the operand.
    copy.push([9]);
    inner == [1, 2] && outer == [[1, 2]] && copy == [[1, 2], [9]]
}

function lms_list_self_spread() -> bool {
    let xs = [1, 2];
    let doubled = [...xs, ...xs];
    xs = [...xs, 3];
    doubled == [1, 2, 1, 2] && xs == [1, 2, 3]
}

test "list_spread_leaves_operand_unchanged" {
    assert.is_true(lms_list_operand_unchanged())
}

test "list_spread_result_does_not_alias_operand" {
    assert.is_true(lms_list_result_does_not_alias_operand())
}

test "list_spread_empty_operands" {
    assert.is_true(lms_list_empty_operands())
}

test "list_spread_is_a_shallow_copy" {
    assert.is_true(lms_list_shallow_copy())
}

test "list_spread_of_itself" {
    assert.is_true(lms_list_self_spread())
}

// ─── Lists: evaluation order ─────────────────────────────────────────────────

function lms_list_evaluates_each_operand_once() -> bool {
    let log = LmsLog { events: [] };
    let out = [...lms_list(log, "a", [1]), ...lms_list(log, "b", [2, 3])];
    log.events == ["a", "b"] && out == [1, 2, 3]
}

function lms_list_evaluates_left_to_right() -> bool {
    let log = LmsLog { events: [] };
    let out = [
        lms_item(log, "1", 1),
        ...lms_list(log, "2", [2]),
        lms_item(log, "3", 3),
        ...lms_list(log, "4", [4]),
        lms_item(log, "5", 5),
    ];
    log.events == ["1", "2", "3", "4", "5"] && out == [1, 2, 3, 4, 5]
}

function lms_grow(xs: int[]) -> int {
    xs.push(99);
    0
}

function lms_list_copies_at_the_spread_position() -> bool {
    let xs = [1, 2];
    // `xs` is copied where the spread stands, before `lms_grow` runs: the
    // element it appends afterwards is not in the result.
    let out = [...xs, lms_grow(xs), ...xs];
    out == [1, 2, 0, 1, 2, 99] && xs == [1, 2, 99]
}

test "list_spread_evaluates_each_operand_once" {
    assert.is_true(lms_list_evaluates_each_operand_once())
}

test "list_spread_evaluates_left_to_right" {
    assert.is_true(lms_list_evaluates_left_to_right())
}

test "list_spread_copies_at_the_spread_position" {
    assert.is_true(lms_list_copies_at_the_spread_position())
}

// ─── Lists: invariance and the fresh result ──────────────────────────────────
// `LmsDog[]` is not an `LmsAnimal[]` (generics are invariant). A spread copies
// the dogs into a new list whose element type is the expected one, so the
// result can hold a cat while the operand stays a list of dogs only.

function lms_dogs() -> LmsDog[] {
    [LmsDog { tag: "a" }, LmsDog { tag: "b" }]
}

function lms_spread_into_interface_list() -> bool {
    let dogs = lms_dogs();
    let animals: LmsAnimal[] = [...dogs];
    animals.push(LmsCat { tag: "c" });
    animals.length() == 3
        && animals[2].name() == "cat:c"
        && animals[0].name() == "dog:a"
        && dogs.length() == 2
        && dogs[1].tag == "b"
}

function lms_spread_into_union_list() -> bool {
    let dogs = lms_dogs();
    let pets: (LmsDog | LmsCat)[] = [LmsCat { tag: "x" }, ...dogs];
    pets.push(LmsCat { tag: "y" });
    pets[1] = LmsCat { tag: "z" };
    pets.length() == 4 && dogs.length() == 2 && dogs[0].tag == "a"
}

function lms_take_animals(animals: LmsAnimal[]) -> int {
    animals.push(LmsCat { tag: "pushed" });
    animals.length()
}

function lms_spread_into_argument() -> bool {
    let dogs = lms_dogs();
    // The callee appends a cat to the list it was given: the fresh one.
    lms_take_animals([...dogs]) == 3 && dogs.length() == 2
}

function lms_spread_into_optional_element_list() -> bool {
    let xs = [1, 2];
    let maybe: int?[] = [...xs, null];
    maybe.push(null);
    maybe.length() == 4 && xs == [1, 2]
}

function lms_spread_into_json() -> bool {
    let xs = [1, 2];
    let names = ["a"];
    // `baml.json.json` has one list arm, `json[]`: the elements are checked
    // against `json`, which an `int` and a `string` both are.
    let j: baml.json.json = [...xs, ...names, null];
    match (j) {
        let items: baml.json.json[] => items.length() == 4,
        _ => false,
    }
}

test "list_spread_into_interface_element_list" {
    assert.is_true(lms_spread_into_interface_list())
}

test "list_spread_into_union_element_list" {
    assert.is_true(lms_spread_into_union_list())
}

test "list_spread_as_argument_does_not_alias" {
    assert.is_true(lms_spread_into_argument())
}

test "list_spread_into_optional_element_list" {
    assert.is_true(lms_spread_into_optional_element_list())
}

test "list_spread_into_json_union" {
    assert.is_true(lms_spread_into_json())
}

// ─── Lists: aliases, generics, narrowing ─────────────────────────────────────

function lms_alias_operand() -> bool {
    let xs: LmsInts = [1, 2];
    let out: LmsInts = [...xs, 3];
    out == [1, 2, 3]
}

function lms_recursive_alias_operand() -> bool {
    let leaf: LmsNest = [];
    let tree: LmsNest = [leaf, leaf];
    let wider: LmsNest = [...tree, []];
    wider.length() == 3 && tree.length() == 2
}

function lms_alias_typed_field_operand() -> bool {
    let holder = LmsHolder {
        values: [1, "two"],
        counts: { "a": 1 },
    };
    let values: baml.json.json[] = [...holder.values, null];
    let counts = { ...holder.counts, "b": 2 };
    values.length() == 3
        && counts.keys() == ["a", "b"]
        && holder.values.length() == 2
        && holder.counts.length() == 1
}

function lms_generic_spread() -> bool {
    let names = lms_copy<string>(["a", "b"]);
    names.push("c");
    lms_concat<int>(0, [1], [2, 3]) == [0, 1, 2, 3] && names.length() == 3
}

function lms_narrowed(xs: int[]?) -> int[] {
    if (xs != null) {
        return [...xs, 0];
    }
    [...(xs ?? [])]
}

function lms_narrowed_operand() -> bool {
    lms_narrowed([1]) == [1, 0] && lms_narrowed(null).length() == 0
}

function lms_captured_operand() -> bool {
    let xs = [1, 2];
    let extend = (x: int) -> int[] { [...xs, x] };
    extend(3) == [1, 2, 3] && xs == [1, 2]
}

test "list_spread_alias_typed_operand" {
    assert.is_true(lms_alias_operand())
}

test "list_spread_recursive_alias_typed_operand" {
    assert.is_true(lms_recursive_alias_operand())
}

test "spread_of_alias_typed_class_field" {
    assert.is_true(lms_alias_typed_field_operand())
}

test "list_spread_in_generic_function" {
    assert.is_true(lms_generic_spread())
}

test "list_spread_of_narrowed_optional" {
    assert.is_true(lms_narrowed_operand())
}

test "list_spread_of_captured_local" {
    assert.is_true(lms_captured_operand())
}

test "list_spread_in_global_and_default" {
    assert.is_true(LMS_EXTENDED == [0, 1, 2, 3]);
    assert.is_true(LMS_BASE == [1, 2]);
    assert.is_true(lms_with_default() == [1, 2, 9]);
    assert.is_true(lms_with_default(xs = [7]) == [7])
}

// ─── Lists: throwing and diverging operands ──────────────────────────────────

function lms_list_throwing_operand() -> bool {
    let log = LmsLog { events: [] };
    let out: int[] = [
        lms_item(log, "before", 1),
        ...lms_explode_list(log),
        lms_item(log, "after", 2),
    ] catch (e) {
        let boom: LmsBoom => {
            log.events.push("caught:" + boom.message);
            []
        },
    };
    // The element before the operand ran, the one after it did not.
    out.length() == 0 && log.events == ["before", "explode", "caught:list"]
}

function lms_list_throws_propagate() -> int[] throws LmsBoom {
    let log = LmsLog { events: [] };
    [0, ...lms_explode_list(log)]
}

function lms_list_throws_reach_the_caller() -> bool {
    let message = lms_list_throws_propagate().length().to_string() catch (e) {
        let boom: LmsBoom => boom.message,
    };
    message == "list"
}

function lms_list_diverging_operand() -> bool {
    let out: int[] = [1, ...lms_diverge(), 2] catch (e) {
        let boom: LmsBoom => [boom.message.length()],
    };
    out == [5]
}

test "list_spread_throwing_operand_is_catchable" {
    assert.is_true(lms_list_throwing_operand())
}

test "list_spread_throws_propagate_to_the_caller" {
    assert.is_true(lms_list_throws_reach_the_caller())
}

test "list_spread_of_a_diverging_operand" {
    assert.is_true(lms_list_diverging_operand())
}

// ─── Maps: entry order and overrides ─────────────────────────────────────────
// A map literal is built by `set`, member by member. `set` on a key that is
// already present replaces the value and keeps the key's first position.

function lms_map_first() -> bool {
    let common = { "type": 1, "id": 2 };
    let out = { ...common, "extra": 3 };
    out.keys() == ["type", "id", "extra"] && out.values() == [1, 2, 3]
}

function lms_map_middle() -> bool {
    let common = { "type": 1, "id": 2 };
    let out = { "a": 0, ...common, "z": 9 };
    out.keys() == ["a", "type", "id", "z"] && out.values() == [0, 1, 2, 9]
}

function lms_map_last() -> bool {
    let common = { "type": 1, "id": 2 };
    let out = { "a": 0, ...common };
    out.keys() == ["a", "type", "id"]
}

function lms_map_only() -> bool {
    let common = { "type": 1, "id": 2 };
    let out = { ...common };
    out.keys() == ["type", "id"] && out == common
}

function lms_map_later_entry_overrides_spread() -> bool {
    let common = { "type": 1, "id": 2 };
    let out = { ...common, "type": 3 };
    // The overridden key keeps the position the spread gave it.
    out.keys() == ["type", "id"] && out["type"] == 3 && out["id"] == 2
}

function lms_map_later_spread_overrides_entry() -> bool {
    let common = { "type": 1, "id": 2 };
    let out = { "id": 0, "z": 9, ...common };
    // `id` was set first, so it stays first; `type` is new and goes last.
    out.keys() == ["id", "z", "type"] && out["id"] == 2 && out["type"] == 1
}

function lms_map_later_spread_overrides_spread() -> bool {
    let a = { "x": 1, "y": 2 };
    let b = { "y": 20, "z": 30 };
    let ab = { ...a, ...b };
    let ba = { ...b, ...a };
    ab.keys() == ["x", "y", "z"]
        && ab.values() == [1, 20, 30]
        && ba.keys() == ["y", "z", "x"]
        && ba.values() == [2, 30, 1]
}

function lms_map_nested() -> bool {
    let a = { "x": 1 };
    let out: map<string, int> = { ...{ ...a, "y": 2 }, ...{} };
    let outer = { "inner": { ...a }, ...{ "copy": { ...a, "z": 3 } } };
    out.keys() == ["x", "y"] && outer["inner"] == a && outer["copy"].length() == 2
}

function lms_map_with_keyword() -> bool {
    let a = { "x": 1 };
    let out = map { ...a, "y": 2 };
    out.keys() == ["x", "y"]
}

function lms_map_many_entries() -> bool {
    let big: map<string, int> = {};
    for (let i = 0; i < 50; i += 1) {
        big["k" + i.to_string()] = i;
    }
    let copy = { "k7": -1, ...big, "k49": -49 };
    copy.length() == 50
        && copy.keys()[0] == "k7"
        && copy["k7"] == 7
        && copy["k49"] == -49
        && copy["k25"] == 25
        && big["k49"] == 49
}

test "map_spread_first_position" {
    assert.is_true(lms_map_first())
}

test "map_spread_middle_position" {
    assert.is_true(lms_map_middle())
}

test "map_spread_last_position_and_trailing_comma" {
    assert.is_true(lms_map_last())
}

test "map_spread_only_member" {
    assert.is_true(lms_map_only())
}

test "map_entry_after_spread_overrides_and_keeps_position" {
    assert.is_true(lms_map_later_entry_overrides_spread())
}

test "map_spread_after_entry_overrides_and_keeps_position" {
    assert.is_true(lms_map_later_spread_overrides_entry())
}

test "map_spread_after_spread_overrides_and_keeps_position" {
    assert.is_true(lms_map_later_spread_overrides_spread())
}

test "map_spread_nested" {
    assert.is_true(lms_map_nested())
}

test "map_spread_in_keyword_map_literal" {
    assert.is_true(lms_map_with_keyword())
}

test "map_spread_copies_every_entry" {
    assert.is_true(lms_map_many_entries())
}

// ─── Maps: the result is a fresh map ─────────────────────────────────────────

function lms_map_operand_unchanged() -> bool {
    let common = { "type": 1, "id": 2 };
    let out = { ...common, "type": 3, "extra": 4 };
    common.keys() == ["type", "id"] && common["type"] == 1 && out.length() == 3
}

function lms_map_result_does_not_alias_operand() -> bool {
    let common = { "type": 1 };
    let copy = { ...common };
    copy["added"] = 2;
    copy["type"] = 100;
    common["later"] = 3;
    common.keys() == ["type", "later"]
        && common["type"] == 1
        && copy.keys() == ["type", "added"]
        && copy["type"] == 100
}

function lms_map_empty_operands() -> bool {
    let none: map<string, int> = {};
    let only_empty: map<string, int> = { ...none };
    let between = { "a": 1, ...none, "b": 2, ...none };
    only_empty.length() == 0 && between.keys() == ["a", "b"]
}

function lms_map_shallow_copy() -> bool {
    let boxes = { "a": LmsBox { items: [1] } };
    let copy = { ...boxes };
    // The value is shared: both maps hold the same box.
    copy["a"].items.push(2);
    // The entry is not: replacing it in the copy leaves the operand.
    copy["a"] = LmsBox { items: [] };
    boxes["a"].items == [1, 2] && copy["a"].items.length() == 0
}

function lms_map_spread_into_wider_value_type() -> bool {
    let dogs = { "a": LmsDog { tag: "a" } };
    let animals: map<string, LmsAnimal> = { ...dogs };
    animals["c"] = LmsCat { tag: "c" };
    let mixed: map<string, int | string> = { ...{ "n": 1 }, "s": "text" };
    mixed["t"] = "more";
    animals.length() == 2
        && animals["c"].name() == "cat:c"
        && dogs.length() == 1
        && mixed.length() == 3
}

test "map_spread_leaves_operand_unchanged" {
    assert.is_true(lms_map_operand_unchanged())
}

test "map_spread_result_does_not_alias_operand" {
    assert.is_true(lms_map_result_does_not_alias_operand())
}

test "map_spread_empty_operands" {
    assert.is_true(lms_map_empty_operands())
}

test "map_spread_is_a_shallow_copy" {
    assert.is_true(lms_map_shallow_copy())
}

test "map_spread_into_wider_value_type_does_not_alias" {
    assert.is_true(lms_map_spread_into_wider_value_type())
}

// ─── Maps: evaluation order ──────────────────────────────────────────────────

function lms_map_evaluates_each_operand_once() -> bool {
    let log = LmsLog { events: [] };
    let out = {
        ...lms_map(log, "a", { "x": 1 }),
        ...lms_map(log, "b", { "y": 2 }),
    };
    log.events == ["a", "b"] && out.keys() == ["x", "y"]
}

function lms_map_evaluates_left_to_right() -> bool {
    let log = LmsLog { events: [] };
    let out = {
        "k1": lms_item(log, "1", 1),
        ...lms_map(log, "2", { "k2": 2 }),
        "k3": lms_item(log, "3", 3),
        ...lms_map(log, "4", { "k4": 4 }),
    };
    log.events == ["1", "2", "3", "4"] && out.keys() == ["k1", "k2", "k3", "k4"]
}

function lms_shrink(m: map<string, int>) -> int {
    m.delete("gone");
    m["late"] = 1;
    0
}

function lms_map_copies_at_the_spread_position() -> bool {
    let m = { "kept": 1, "gone": 2 };
    // `m` is copied where the spread stands, before `lms_shrink` changes it.
    let out = { ...m, "call": lms_shrink(m) };
    out.keys() == ["kept", "gone", "call"] && m.keys() == ["kept", "late"]
}

test "map_spread_evaluates_each_operand_once" {
    assert.is_true(lms_map_evaluates_each_operand_once())
}

test "map_spread_evaluates_left_to_right" {
    assert.is_true(lms_map_evaluates_left_to_right())
}

test "map_spread_copies_at_the_spread_position" {
    assert.is_true(lms_map_copies_at_the_spread_position())
}

// ─── Maps: the copy is one state of the operand ──────────────────────────────
// A spread takes every entry of its operand in one read, then sets them into
// the new map. Setting a key runs its `hash`, which is user code here and
// changes the operand while the copy is under way. What was copied does not
// change: it is the operand as it was at the spread, each key with the value
// it had then.

class LmsTamper {
    targets: map<LmsTamperKey, int>[],
    armed: bool,
    hashed: int,
}

class LmsTamperKey {
    id: int,
    tamper: LmsTamper,

    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never {
            self.id == other.id
        }
    }

    implements baml.Hash {
        function hash(self, state: baml.hash.Hasher) -> void throws never {
            self.tamper.hashed += 1;
            if (self.tamper.armed) {
                // Disarm first: emptying the map must not re-enter here.
                self.tamper.armed = false;
                self.tamper.targets[0].clear();
            }
            self.id.hash(state);
        }
    }
}

function lms_map_copy_is_one_state_of_the_operand() -> bool {
    let tamper = LmsTamper {
        targets: [],
        armed: false,
        hashed: 0,
    };
    let first = LmsTamperKey {
        id: 1,
        tamper: tamper,
    };
    let second = LmsTamperKey {
        id: 2,
        tamper: tamper,
    };
    let third = LmsTamperKey {
        id: 3,
        tamper: tamper,
    };
    let source: map<LmsTamperKey, int> = {};
    source[first] = 10;
    source[second] = 20;
    source[third] = 30;
    tamper.targets.push(source);
    tamper.hashed = 0;
    tamper.armed = true;
    // The first `set` of the copy hashes `first`, which empties `source`.
    let copy = { ...source };
    let ids = copy.keys().map((key: LmsTamperKey) -> int { key.id });
    tamper.armed == false
        && source.length() == 0
        && ids == [1, 2, 3]
        && copy.values() == [10, 20, 30]
        && copy[second] == 20
}

test "map_spread_copy_is_one_state_of_the_operand" {
    assert.is_true(lms_map_copy_is_one_state_of_the_operand())
}

// ─── Maps: key types, aliases, generics, narrowing ───────────────────────────

function lms_map_int_keys() -> bool {
    let by_int = { 1: "a", 2: "b" };
    let out = { 2: "z", ...by_int, 3: "c" };
    out.keys() == [2, 1, 3] && out[2] == "b"
}

function lms_map_enum_keys() -> bool {
    let warm = { LmsColor.Red: 1 };
    let out = { ...warm, LmsColor.Blue: 3, ...{ LmsColor.Red: 10 } };
    out.keys() == [LmsColor.Red, LmsColor.Blue] && out[LmsColor.Red] == 10
}

function lms_map_literal_key_type() -> bool {
    let narrow: map<"a" | "b", int> = { "a": 1 };
    // A literal-keyed operand fits a literal-keyed target and a wider one.
    let same: map<"a" | "b", int> = { ...narrow, "b": 2 };
    let wide: map<string, int> = { ...narrow, "anything": 3 };
    same.length() == 2 && wide.keys() == ["a", "anything"]
}

function lms_map_alias_operand() -> bool {
    let base: LmsDict = { "a": 1 };
    let out: LmsDict = { ...base, "b": 2 };
    out.keys() == ["a", "b"]
}

function lms_map_generic_spread() -> bool {
    let merged = lms_merge<string, int>({ "a": 1, "b": 2 }, { "b": 20, "c": 30 });
    let by_int = lms_merge<int, string>({ 1: "a" }, { 2: "b" });
    merged.keys() == ["a", "b", "c"] && merged["b"] == 20 && by_int.keys() == [1, 2]
}

function lms_map_narrowed(m: map<string, int>?) -> map<string, int> {
    if (m != null) {
        return { ...m, "seen": 1 };
    }
    { ...(m ?? {}) }
}

function lms_map_narrowed_operand() -> bool {
    lms_map_narrowed({ "a": 1 }).keys() == ["a", "seen"] && lms_map_narrowed(null).length() == 0
}

function lms_map_in_match_arm(n: int, m: map<string, int>) -> map<string, int> {
    // An arm's `{` opens a block, so a map literal there is parenthesized.
    match (n) {
        0 => ({ ...m }),
        _ => ({ ...m, "n": n }),
    }
}

test "map_spread_int_keys" {
    assert.is_true(lms_map_int_keys())
}

test "map_spread_enum_keys" {
    assert.is_true(lms_map_enum_keys())
}

test "map_spread_literal_key_types" {
    assert.is_true(lms_map_literal_key_type())
}

test "map_spread_alias_typed_operand" {
    assert.is_true(lms_map_alias_operand())
}

test "map_spread_in_generic_function" {
    assert.is_true(lms_map_generic_spread())
}

test "map_spread_of_narrowed_optional" {
    assert.is_true(lms_map_narrowed_operand())
}

test "map_spread_in_match_arm" {
    assert.is_true(lms_map_in_match_arm(0, { "k": 1 }).keys() == ["k"]);
    assert.is_true(lms_map_in_match_arm(2, { "k": 1 }).keys() == ["k", "n"])
}

test "map_spread_in_global" {
    assert.is_true(LMS_TUNED.keys() == ["retries", "timeout"]);
    assert.is_true(LMS_TUNED["timeout"] == 5);
    assert.is_true(LMS_DEFAULTS["timeout"] == 30)
}

// ─── Maps: throwing and diverging operands ───────────────────────────────────

function lms_map_throwing_operand() -> bool {
    let log = LmsLog { events: [] };
    let out: map<string, int> = {
        "before": lms_item(log, "before", 1),
        ...lms_explode_map(log),
        "after": lms_item(log, "after", 2),
    } catch (e) {
        let boom: LmsBoom => {
            log.events.push("caught:" + boom.message);
            {}
        },
    };
    out.length() == 0 && log.events == ["before", "explode", "caught:map"]
}

function lms_map_diverging_operand() -> bool {
    let out: map<string, int> = { "a": 1, ...lms_diverge() } catch (e) {
        let boom: LmsBoom => ({ "caught": boom.message.length() }),
    };
    out.keys() == ["caught"]
}

test "map_spread_throwing_operand_is_catchable" {
    assert.is_true(lms_map_throwing_operand())
}

test "map_spread_of_a_diverging_operand" {
    assert.is_true(lms_map_diverging_operand())
}
