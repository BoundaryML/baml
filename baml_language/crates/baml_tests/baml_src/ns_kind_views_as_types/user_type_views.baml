// A class of the caller's own that implements `reflect.TypeView` goes
// wherever a `reflect.Type` does: every reflection API that takes a
// `TypeView` calls the class's own `as_type()`.

/// Views whatever type it holds.
class UvShown {
    ty: reflect.Type,

    implements reflect.TypeView {
        function as_type(self) -> reflect.Type throws never {
            self.ty
        }
    }
}

/// Views `int`.
class UvInt {
    implements reflect.TypeView {
        function as_type(self) -> reflect.Type throws never {
            int.type()
        }
    }
}

interface UvNamed {
    name: string,
}

interface UvSized {
    function size(self) -> int throws never;
}

class UvBox {
    n: int,

    implements UvSized {
        function size(self) -> int throws never {
            self.n
        }
    }
}

/// Views `UvBox`, which implements `UvSized`.
class UvBoxView {
    implements reflect.TypeView {
        function as_type(self) -> reflect.Type throws never {
            UvBox.type()
        }
    }
}

function uv_shown(ty: reflect.Type) -> UvShown {
    UvShown { ty: ty }
}

test "a class field takes a user view" {
    let row = reflect.Class.new(
        "UvRow",
        {
            "count": UvInt {},
            "label": uv_shown(string.type()),
            "note": string.type().meta(description = "a note"),
        },
    );
    let fields = row.fields();
    assert.equal(fields[0].type, int.type());
    assert.equal(fields[1].type, string.type());
    assert.equal(fields[2].type, string.type());
    assert.equal(fields[2].meta.description, "a note")
}

test "extends takes a user view for a field" {
    let person = reflect.Class.extends<UvNamed>("UvPerson", { "age": UvInt {} });
    let fields = person.fields();
    assert.equal(fields[0].name, "name");
    assert.equal(fields[1].name, "age");
    assert.equal(fields[1].type, int.type());
    assert.is_true(person.implements(reflect.Type.of<UvNamed>()))
}

test "a union takes user views" {
    let union = reflect.Union.new([UvInt {}, uv_shown(string.type()), bool.type()]);
    assert.equal(union.as_type(), reflect.Type.of<int | string | bool>())
}

test "a map takes user views for its key and value" {
    assert.equal(
        reflect.Map.new(uv_shown(string.type()), UvInt {}).as_type(),
        reflect.Type.of<map<string, int>>(),
    )
}

test "a builder field and a pending union take user views" {
    let node = reflect.Class.builder("UvNode");
    node.field("count", UvInt {});
    node.field("next", node.type().union([UvInt {}]));
    let fields = node.build().fields();
    assert.equal(fields[0].type, int.type());
    assert.equal(fields[1].type.to_string(), "UvNode | int")
}

test "with_types mounts a user view" {
    let app = reflect.Package.compile({ "empty.baml": "" }).with_types({ "Count": UvInt {} });
    let consumer = reflect.Package.compile(
        { "consumer.baml": "function twice(x: app.Count) -> int { x + x }" },
        packages = { "app": app },
    );
    let twice = consumer.get_function<(int) -> int>("root.twice");
    assert.equal(twice(4), 8)
}

test "implements and implemented_by take user views on both sides" {
    let sized = reflect.Type.of<UvSized>();
    let sized_kind = sized.as_interface() ?? throw "UvSized is an interface";
    let box_kind = UvBox.type().as_class() ?? throw "UvBox is a class";
    let sized_view = uv_shown(sized);
    let box_view = uv_shown(UvBox.type());

    // A user view as the argument.
    assert.is_true(UvBox.type().implements(sized_view));
    assert.is_true(box_kind.implements(sized_view));
    assert.is_true(sized.implemented_by(box_view));
    assert.is_true(sized_kind.implemented_by(box_view));
    // A user view as the receiver, through `TypeView`'s default methods.
    assert.is_true(box_view.implements(sized));
    assert.is_true(box_view.implements(sized_view));
    assert.is_true(sized_view.implemented_by(UvBox.type()));
    assert.is_true(sized_view.implemented_by(box_view));
    // A user view of a type that does not implement the interface.
    assert.equal(UvInt {}.implements(sized_view), false);
    assert.equal(sized_view.implemented_by(UvInt {}), false);
    assert.equal(sized.implemented_by(UvInt {}), false)
}

test "a user view of int behaves like reflect.Type.of<int>()" {
    let view = UvInt {};
    let int_type = reflect.Type.of<int>();
    assert.equal(view.as_type(), int_type);
    assert.equal(view.array().as_type(), int_type.array().as_type());
    assert.equal(view.optional().as_type(), int_type.optional().as_type());
    assert.is_true(view.kind() is reflect.Primitive);
    assert.equal(view.meta(description = "n").ty, int_type);
    assert.equal(reflect.Union.new([view]).as_type(), reflect.Union.new([int_type]).as_type());
    assert.equal(
        reflect.Map.new(string.type(), view).as_type(),
        reflect.Map.new(string.type(), int_type).as_type(),
    );
    assert.equal(
        reflect.Class.new("UvSame", { "x": view }).to_baml(),
        reflect.Class.new("UvSame", { "x": int_type }).to_baml(),
    )
}

// `let type` takes any `reflect.TypeView` too, and binds the type the view's
// own `as_type()` returns.

function uv_bound(view: reflect.TypeView) -> string {
    let type T extends UvSized = view else {
        return "missed";
    };
    "bound"
}

function uv_bound_size(view: reflect.TypeView, json: string) -> int {
    let type T extends UvSized = view else {
        return -1;
    };
    baml.json.decode<T>(json).size()
}

function uv_type_name<V extends reflect.TypeView>(view: V) -> string {
    let type T = view;
    reflect.Type.of<T>().to_string()
}

test "let type binds a user view" {
    let type T = UvInt {};
    assert.equal(reflect.Type.of<T>(), int.type());
    let parsed: unknown = baml.json.to<T>(baml.json.parse("7"));
    assert.equal(parsed, 7)
}

test "let type binds a value typed reflect.TypeView" {
    let view: reflect.TypeView = uv_shown(string.type());
    let type T = view;
    assert.equal(reflect.Type.of<T>(), string.type())
}

test "let type binds a type parameter bounded by reflect.TypeView" {
    assert.equal(uv_type_name(UvInt {}), "int");
    assert.equal(uv_type_name(string.type()), "string")
}

test "let type binds a union of a user view and a type" {
    let views: (UvInt | reflect.Type)[] = [UvInt {}, string.type()];
    let names: string[] = [];
    for (let view in views) {
        let type T = view;
        names.push(reflect.Type.of<T>().to_string());
    }
    assert.equal(names.join(","), "int,string")
}

test "a bounded let type takes a user view" {
    let type T extends UvSized = UvBoxView {} else {
        throw "UvBoxView's type implements UvSized";
    };
    let boxed = baml.json.decode<T>(`{"n": 4}`);
    assert.equal(boxed.size(), 4);
    assert.equal(uv_bound_size(UvBoxView {}, `{"n": 5}`), 5)
}

test "a bounded let type checks the type a user view returns" {
    assert.equal(uv_bound(UvBoxView {}), "bound");
    assert.equal(uv_bound(UvInt {}), "missed");
    assert.equal(uv_bound_size(UvInt {}, "1"), -1)
}
