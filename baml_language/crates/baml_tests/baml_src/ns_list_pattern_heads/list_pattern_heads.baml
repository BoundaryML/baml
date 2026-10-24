// A list pattern matches any list of the right length, whatever its element
// type: generics are invariant, so an `int[]` is not an `unknown[]`, and the
// test is the value's list head and length, not membership in one list type.

function lph_one<T>(x: T) -> int {
    match (x) {
        [let a] => 1,
        _ => 0,
    }
}

test "a list pattern matches a list held by a type parameter" {
    assert.equal(lph_one([1]), 1);
    assert.equal(lph_one(["s"]), 1);
    assert.equal(lph_one(3), 0)
}

function lph_pair(x: unknown) -> int {
    match (x) {
        [let a, let b] => 1,
        _ => 0,
    }
}

test "a list pattern on unknown tests the list head" {
    assert.equal(lph_pair([1, 2]), 1);
    assert.equal(lph_pair("ab"), 0);
    assert.equal(lph_pair(3), 0);
    assert.equal(lph_pair({"a": 1, "b": 2}), 0)
}

function lph_rest(x: unknown) -> string {
    match (x) {
        [..] => "list",
        let rest => "other",
    }
}

test "a rest pattern on unknown does not consume the non-lists" {
    assert.equal(lph_rest([]), "list");
    assert.equal(lph_rest("s"), "other")
}

function lph_lists(x: int[] | string[] | null) -> int {
    match (x) {
        [] => 1,
        [let a] => 2,
        _ => 0,
    }
}

test "a list pattern over a union of list types matches each" {
    let empty: string[] = [];
    assert.equal(lph_lists(empty), 1);
    assert.equal(lph_lists(["s"]), 2);
    assert.equal(lph_lists([1]), 2);
    assert.equal(lph_lists([1, 2]), 0);
    assert.equal(lph_lists(null), 0)
}

function lph_cover(x: int[] | string[]) -> int {
    match (x) {
        [] => 1,
        [_, ..] => 2,
    }
}

test "list patterns cover a union of list types" {
    let empty: int[] = [];
    assert.equal(lph_cover(empty), 1);
    assert.equal(lph_cover(["s"]), 2)
}

function lph_claim(x: int[] | string[]) -> string {
    match (x) {
        [let a: int] => "int",
        [let b: string] => "string",
        _ => "other",
    }
}

test "a list pattern's element ascription claims one list member" {
    assert.equal(lph_claim([1]), "int");
    assert.equal(lph_claim(["s"]), "string");
    assert.equal(lph_claim([1, 2]), "other")
}

function lph_open_member<T>(x: int[] | T) -> int {
    match (x) {
        [let a] => 1,
        _ => 0,
    }
}

test "a list pattern matches the lists a type parameter member holds" {
    assert.equal(lph_open_member<string[]>(["s"]), 1);
    assert.equal(lph_open_member<string[]>([1]), 1);
    assert.equal(lph_open_member<string>("s"), 0)
}

type LphTree = int | LphTree[]

function lph_tree(x: LphTree) -> int {
    match (x) {
        [let a, ..] => 1,
        let n: int => 2,
        [] => 3,
    }
}

test "list patterns match a recursive alias's list member" {
    let leaf: LphTree[] = [];
    assert.equal(lph_tree([1, [2]]), 1);
    assert.equal(lph_tree(5), 2);
    assert.equal(lph_tree(leaf), 3)
}

function lph_is(x: string | int[]) -> bool {
    x is [let a]
}

function lph_or(x: unknown) -> int {
    match (x) {
        [let a] | [let a, _] => 1,
        _ => 0,
    }
}

test "is and or-patterns test the list head" {
    assert.is_true(!lph_is("s"));
    assert.is_true(lph_is([1]));
    assert.equal(lph_or([1, 2]), 1);
    assert.equal(lph_or({"a": 1}), 0)
}

function lph_is_value(x: int | string) -> bool {
    x is []
}

test "a list pattern written as a value against a non-list is false" {
    assert.is_true(!lph_is_value(1));
    assert.is_true(!lph_is_value("s"))
}

function lph_or_alternative(x: int | string[]) -> int {
    match (x) {
        [] | [_] => 1,
        _ => 0,
    }
}

test "an or-alternative list pattern matches the lists among a union" {
    assert.equal(lph_or_alternative(["a"]), 1);
    assert.equal(lph_or_alternative(3), 0);
    assert.equal(lph_or_alternative(["a", "b"]), 0)
}

class LphA {
    value int
}

function lph_class_in_union_list(x: (LphA | string)[] | LphA[]) -> string {
    match (x) {
        [LphA { value }] => `hit ${value}`,
        _ => "miss",
    }
}

test "a class sub-pattern claims every list whose elements may be the class" {
    let mixed: (LphA | string)[] = [LphA { value: 1 }];
    let strings: (LphA | string)[] = ["s"];
    assert.equal(lph_class_in_union_list(mixed), "hit 1");
    assert.equal(lph_class_in_union_list(strings), "miss");
    assert.equal(lph_class_in_union_list([LphA { value: 2 }]), "hit 2")
}

function lph_class_beside_a_param<T>(x: (LphA | string)[] | T, d: T) -> T {
    match (x) {
        [LphA { value }] => d,
        _ => d,
    }
}

test "a list pattern beside a type parameter still binds against the class" {
    let mixed: (LphA | string)[] = [LphA { value: 1 }];
    assert.equal(lph_class_beside_a_param<string>(mixed, "d"), "d");
    assert.equal(lph_class_beside_a_param<string>("s", "d"), "d")
}
