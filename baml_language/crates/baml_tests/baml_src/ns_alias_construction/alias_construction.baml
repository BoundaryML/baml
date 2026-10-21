// A constructor head is legal wherever the type it denotes would be: an
// alias of a class constructs that class, with the type arguments the
// alias pins, and an alias of an alias constructs the class the chain ends
// in. The object IS the class; no second identity rides the alias.

class Point {
    x int
    y int
}

class Pair<T> {
    first T
    second T
}

type PointAlias = Point
type IntPair = Pair<int>
type PointAliasAlias = PointAlias

function build_point() -> Point throws never {
    PointAlias { x: 1, y: 2 }
}

test "alias_of_class_constructs_the_class" {
    let p = PointAlias { x: 1, y: 2 };
    assert.equal(p.x + p.y, 3);
    assert.is_true(build_point() is Point)
}

test "alias_pins_the_class_type_arguments" {
    let pair = IntPair { first: 4, second: 5 };
    assert.equal(pair.first + pair.second, 9)
}

test "alias_of_alias_constructs_the_class_it_ends_in" {
    let p = PointAliasAlias { x: 7, y: 8 };
    assert.equal(p.y, 8)
}
