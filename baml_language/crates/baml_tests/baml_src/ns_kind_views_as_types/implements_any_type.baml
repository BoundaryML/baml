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

interface Source {
    type Item;

    function first(self) -> Self.Item throws never;
}

class IntSource {
    value: int,

    implements Source {
        type Item = int;

        function first(self) -> int throws never {
            self.value
        }
    }
}

class StringSource {
    value: string,

    implements Source {
        type Item = string;

        function first(self) -> string throws never {
            self.value
        }
    }
}

/// `Error` defaults to `never`.
interface Fallible {
    type Error = never;

    function run(self) -> int throws Self.Error;
}

class Safe {
    implements Fallible {
        function run(self) -> int throws never {
            1
        }
    }
}

class Risky {
    implements Fallible {
        type Error = string;

        function run(self) -> int throws string {
            throw "risky"
        }
    }
}

interface Holder<T> {
    function held(self) -> T throws never;
}

class IntHolder {
    implements Holder<int> {
        function held(self) -> int throws never {
            1
        }
    }
}

class StringHolder {
    implements Holder<string> {
        function held(self) -> string throws never {
            "a"
        }
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

// An interface built with `reflect.Type.of` pins its associated types, and an
// omitted one takes its default, so only an implementation with exactly those
// types matches.
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

// An interface looked up by name leaves its associated types open, so it
// matches any implementation; `reflect.Type.of` names them.
test "an interface looked up by name matches any implementation" {
    let source = reflect.Package.current().get_interface("kind_views_as_types.Source");
    assert.is_true(IntSource.type().implements(source));
    assert.is_true(StringSource.type().implements(source));
    assert.equal(Circle.type().implements(source), false);
    assert.is_true(source.implemented_by(IntSource.type()));
    assert.is_true(source.implemented_by(StringSource.type()));
    assert.equal(source.implemented_by(Circle.type()), false)
}

test "an interface from reflect.Type.of matches only its associated types" {
    let int_source = reflect.Type.of<Source<Item = int>>();
    assert.is_true(IntSource.type().implements(int_source));
    assert.equal(StringSource.type().implements(int_source), false);
    assert.is_true(int_source.implemented_by(IntSource.type()));
    assert.equal(int_source.implemented_by(StringSource.type()), false)
}

test "a defaulted associated type is open by name and the default with reflect.Type.of" {
    let by_name = reflect.Package.current().get_interface("kind_views_as_types.Fallible");
    let defaulted = reflect.Type.of<Fallible>();
    assert.equal(defaulted.to_string(), "kind_views_as_types.Fallible<Error = never>");
    assert.is_true(Safe.type().implements(by_name));
    assert.is_true(Risky.type().implements(by_name));
    assert.is_true(by_name.implemented_by(Risky.type()));
    assert.is_true(Safe.type().implements(defaulted));
    assert.equal(Risky.type().implements(defaulted), false);
    assert.equal(defaulted.implemented_by(Risky.type()), false);
    assert.is_true(Risky.type().implements(reflect.Type.of<Fallible<Error = string>>()))
}

test "a generic interface looked up by name matches every instantiation" {
    let by_name = reflect.Package.current().get_interface("kind_views_as_types.Holder");
    assert.is_true(IntHolder.type().implements(by_name));
    assert.is_true(StringHolder.type().implements(by_name));
    assert.is_true(IntHolder.type().implements(reflect.Type.of<Holder<int>>()));
    assert.equal(StringHolder.type().implements(reflect.Type.of<Holder<int>>()), false)
}

test "a standard-library interface is not found by name" {
    assert.equal(reflect.Package.current().find_interface("baml.iter.Iterator"), null)
}
