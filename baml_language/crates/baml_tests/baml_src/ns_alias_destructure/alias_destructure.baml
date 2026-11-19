// A destructure head is legal wherever the class it denotes would be, as a
// constructor head is: an alias of a class destructures that class, with
// the type arguments the alias pins, and an alias chain destructures the
// class it ends in. The runtime test and the field reads are the class's.

class AdPoint {
    x int
    y int
}

class AdOther {
    x int
}

class AdBox<T> {
    value T
}

type AdPointAlias = AdPoint
type AdPointAliasAlias = AdPointAlias
type AdIntBox = AdBox<int>

function ad_sum(v: AdPoint | AdOther) -> int throws never {
    match (v) {
        AdPointAlias { x: let a, y: let b } => a + b,
        AdOther { x: let a } => a * 100,
    }
}

function ad_chain_y(v: AdPoint) -> int throws never {
    match (v) {
        AdPointAliasAlias { y: let b } => b,
    }
}

function ad_unbox(b: AdBox<int>) -> int throws never {
    match (b) {
        AdIntBox { value: let n } => n,
    }
}

function ad_first_x_is_one(v: AdPoint | AdOther) -> string throws never {
    match (v) {
        AdPointAlias { x: 1 } => "point one",
        AdPointAlias { x: _ } => "point",
        AdOther { x: _ } => "other",
    }
}

test "alias_destructure_matches_the_class_and_reads_its_fields" {
    assert.equal(ad_sum(AdPoint { x: 1, y: 2 }), 3)
}

test "alias_destructure_falls_through_for_another_class" {
    assert.equal(ad_sum(AdOther { x: 4 }), 400)
}

test "alias_chain_destructures_the_class_it_ends_in" {
    assert.equal(ad_chain_y(AdPoint { x: 7, y: 8 }), 8)
}

test "generic_target_alias_destructures_at_its_pinned_args" {
    assert.equal(ad_unbox(AdBox<int> { value: 5 }), 5)
}

test "alias_destructure_field_tests_fall_through_to_later_arms" {
    assert.equal(ad_first_x_is_one(AdPoint { x: 1, y: 0 }), "point one");
    assert.equal(ad_first_x_is_one(AdPoint { x: 2, y: 0 }), "point");
    assert.equal(ad_first_x_is_one(AdOther { x: 1 }), "other")
}
