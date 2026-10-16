// `implements` and `implemented_by` answer for any concrete type, not only
// classes: an enum, a primitive, a list or a map implements an interface
// through an `implement ... for` block.

interface Sided {
    function side_count(self) -> int throws never;
}

class Square {
    side: int,

    implements Sided {
        function side_count(self) -> int throws never {
            4
        }
    }
}

class Circle {
    radius: int,
}

enum Polygon {
    Triangle,
    Pentagon,
}

implement Sided for Polygon {
    function side_count(self) -> int throws never {
        match (self) {
            Polygon.Triangle => 3,
            Polygon.Pentagon => 5,
        }
    }
}

implement Sided for int {
    function side_count(self) -> int throws never {
        self
    }
}

implement Sided for string[] {
    function side_count(self) -> int throws never {
        self.length()
    }
}

implement Sided for map<string, int> {
    function side_count(self) -> int throws never {
        self.length()
    }
}

/// An int iterator that can throw: its `Error` is not the default `never`.
class Flaky {
    calls: int,

    implements baml.iter.Iterable {
        type Item = int;
        type Error = baml.errors.InvalidArgument;

        function iter(
            self,
        ) -> baml.iter.Iterator<Item = int, Error = baml.errors.InvalidArgument> throws never {
            self
        }
    }

    implements baml.iter.Iterator {
        type Item = int;
        type Error = baml.errors.InvalidArgument;

        function next(self) -> int | baml.iter.Done throws baml.errors.InvalidArgument {
            throw baml.errors.InvalidArgument { message: "flaky" }
        }
    }
}

test "a class, an enum, a primitive, a list and a map implement an interface" {
    let sided = reflect.Type.of<Sided>();
    assert.is_true(Square.type().implements(sided));
    assert.is_true(Polygon.type().implements(sided));
    assert.is_true(int.type().implements(sided));
    assert.is_true(reflect.Type.of<string[]>().implements(sided));
    assert.is_true(reflect.Type.of<map<string, int>>().implements(sided));
    assert.equal(Circle.type().implements(sided), false);
    assert.equal(float.type().implements(sided), false);
    assert.equal(reflect.Type.of<int[]>().implements(sided), false)
}

test "implemented_by takes any concrete type" {
    let sided = reflect.Type.of<Sided>().as_interface() ?? throw "Sided is an interface";
    assert.is_true(sided.implemented_by(Square.type()));
    assert.is_true(sided.implemented_by(Polygon.type()));
    assert.is_true(sided.implemented_by(int.type()));
    assert.is_true(sided.implemented_by(reflect.Type.of<string[]>()));
    assert.equal(sided.implemented_by(Circle.type()), false);
    // An interface is not a concrete type, so it does not implement itself.
    assert.equal(sided.implemented_by(sided), false)
}

test "implements runs on the kind view of any concrete type" {
    let sided = reflect.Type.of<Sided>();
    let polygon = Polygon.type().as_enum() ?? throw "Polygon is an enum";
    let number = int.type().as_primitive() ?? throw "int is a primitive";
    let names = reflect.Type.of<string[]>().as_array() ?? throw "string[] is an array";
    let counts = reflect.Type.of<map<string, int>>().as_map() ?? throw "a map";
    assert.is_true(polygon.implements(sided));
    assert.is_true(number.implements(sided));
    assert.is_true(names.implements(sided));
    assert.is_true(counts.implements(sided))
}

// The interface operand pins its associated types, and an omitted one takes
// its default, so only an implementation with exactly those types matches.
test "implements matches the interface's associated types exactly" {
    let range = reflect.Type.of<baml.iter.Range>();
    let flaky = Flaky.type();
    let int_iterator = reflect.Type.of<baml.iter.Iterator<Item = int>>();
    assert.is_true(range.implements(int_iterator));
    assert.is_true(
        range.implements(reflect.Type.of<baml.iter.Iterator<Item = int, Error = never>>()),
    );
    assert.equal(range.implements(reflect.Type.of<baml.iter.Iterator<Item = string>>()), false);
    // `Item = int` alone means `Error = never`, which a throwing iterator is not.
    assert.equal(flaky.implements(int_iterator), false);
    assert.is_true(
        flaky.implements(
            reflect.Type.of<baml.iter.Iterator<Item = int, Error = baml.errors.InvalidArgument>>(),
        ),
    )
}
