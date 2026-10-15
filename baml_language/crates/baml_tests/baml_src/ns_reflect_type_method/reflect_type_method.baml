// `X.type()`: the static method every type gets from `reflect.Reflectable`,
// the short form of `reflect.Type.of<X>()`.

class Point {
    x: int,
    y: int,
}

enum Suit {
    Hearts,
    Spades,
}

class Pair<A, B> {
    first: A,
    second: B,
}

// A class that declares its own instance `type` method keeps it.
class OwnInstanceType {
    function type(self) -> string {
        "own instance"
    }
}

// A class that declares its own static `type` method keeps it.
class OwnStaticType {
    function type() -> string {
        "own static"
    }
}

// A field called `type` stays a field.
class TypeField {
    type: string,
}

// An interface that declares an instance `type`; its implementors keep it,
// whether the implementation is a separate block or inside the class.
interface Shape {
    function type(self) -> string throws never;
}

class Circle {
    r: int,
}

implement Shape for Circle {
    function type(self) -> string {
        "circle"
    }
}

class Square {
    side: int,

    implements Shape {
        function type(self) -> string {
            "square"
        }
    }
}

// An interface that declares a static `type`; its implementors keep it.
interface Kind {
    function type() -> string throws never;
}

class Leaf {
    v: int,
}

implement Kind for Leaf {
    function type() -> string {
        "leaf"
    }
}

function shape_type<S extends Shape>(shape: S) -> string {
    shape.type()
}

function kind_type<K extends Kind>() -> string {
    K.type()
}

testset "type method" {
    test "int" {
        assert.equal(int.type().to_string(), "int")
    }

    test "string" {
        assert.equal(string.type().to_string(), "string")
    }

    test "class" {
        assert.equal(Point.type().to_string(), "reflect_type_method.Point")
    }

    test "enum" {
        assert.equal(Suit.type().to_string(), "reflect_type_method.Suit")
    }

    test "generic class" {
        assert.equal(
            Pair<int, string[]>.type().to_string(),
            "reflect_type_method.Pair<int, string[]>",
        )
    }

    test "namespaced class" {
        assert.equal(
            root.reflect_type_method.models.Base.type().to_string(),
            "reflect_type_method.models.Base",
        )
    }

    test "equals reflect.Type.of" {
        assert.is_true(int.type() == reflect.Type.of<int>());
        assert.is_true(string.type() == reflect.Type.of<string>());
        assert.is_true(Point.type() == reflect.Type.of<Point>());
        assert.is_true(Suit.type() == reflect.Type.of<Suit>());
        assert.is_true(Pair<int, string[]>.type() == reflect.Type.of<Pair<int, string[]>>());
        assert.is_true(
            root.reflect_type_method.models.Base.type()
                == reflect.Type.of<root.reflect_type_method.models.Base>(),
        );
    }

    test "generic arguments are part of the type" {
        assert.is_true(Pair<int, int>.type() != Pair<int, string>.type())
    }

    test "returns a reflect.Type" {
        let point: reflect.Type = Point.type();
        let view = point.as_class() ?? throw "expected a class";
        assert.equal(view.fields().length(), 2)
    }
}

testset "type method keeps existing type members" {
    test "AnyClass.type() is the value's runtime type" {
        let value: reflect.AnyClass = Point { x: 1, y: 2 };
        assert.is_true(value.type() == Point.type())
    }

    test "class.Builder.type() is the builder's pending type" {
        let node = reflect.class.builder("TypeMethodNode");
        let next: reflect.class.PendingType = node.type().optional();
        node.field("next", next);
        let built = node.build().as_type().as_class() ?? throw "expected class";
        let field = built.fields().at(0) ?? throw "expected next field";
        assert.equal(field.name, "next")
    }

    test "a class's own instance type method" {
        assert.equal(OwnInstanceType {}.type(), "own instance")
    }

    test "a class's own static type method" {
        assert.equal(OwnStaticType.type(), "own static")
    }

    test "a field called type" {
        assert.equal(TypeField { type: "field" }.type, "field")
    }

    test "a reflected field's type" {
        let view = Point.type().as_class() ?? throw "expected a class";
        let field = view.fields().at(0) ?? throw "expected a field";
        assert.is_true(field.type == int.type())
    }

    test "an interface instance method called type" {
        assert.equal(Circle { r: 1 }.type(), "circle");
        assert.equal(Square { side: 1 }.type(), "square");
        let shape: Shape = Circle { r: 2 };
        assert.equal(shape.type(), "circle");
        let either: Circle | Square = Square { side: 2 };
        assert.equal(either.type(), "square");
        assert.equal(shape_type(Square { side: 3 }), "square")
    }

    test "an interface static method called type" {
        assert.equal(Leaf.type(), "leaf");
        assert.equal(kind_type<Leaf>(), "leaf")
    }
}
