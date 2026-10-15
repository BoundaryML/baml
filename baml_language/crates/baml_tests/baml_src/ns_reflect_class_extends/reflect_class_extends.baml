// reflect.Class.extends<I>(name, fields): a runtime class that implements `I`
// and every interface `I` requires, taking their fields and linking each one
// to the class field of the same name.

interface ExtFoo {
    a: int,
    b: int,
}

interface ExtHasA {
    /// The a field.
    a: int,
}

interface ExtHasB {
    b: string,
}

interface ExtFooBar requires ExtHasA, ExtHasB {
    c: bool,
}

interface ExtLeft requires ExtHasA {
    left: int,
}

interface ExtRight requires ExtHasA {
    right: int,
}

interface ExtDiamond requires ExtLeft, ExtRight {}

interface ExtAlsoA {
    a: int,
}

interface ExtSharesA requires ExtHasA, ExtAlsoA {}

interface ExtStringA {
    a: string,
}

interface ExtConflict requires ExtHasA, ExtStringA {}

interface ExtBox<T> {
    value: T,
}

interface ExtNamedBox<T> requires ExtBox<T> {
    name: string,
}

interface ExtKeyed {
    type Key;
    key: Self.Key,
}

interface ExtGreeter {
    name: string,
    function greet(self) -> string throws never {
        "hello, " + self.name
    }
}

interface ExtDescribed {
    name: string,
    function describe(self) -> string throws never;
}

interface ExtRequiresDescribed requires ExtDescribed {}

class ExtNotAnInterface {
    a: int,
}

function ext_field_names(class_t: reflect.Class) -> string {
    let names = "";
    for (let field in class_t.fields()) {
        if (names != "") {
            names += ",";
        }
        names += field.name;
    }
    names
}

test "extends_adds_the_interface_fields_then_the_given_ones" {
    let foo_t = reflect.Class.extends<ExtFoo>("Foo", { "c": int.type() });
    let type Foo = foo_t.as_type();
    let foo: ExtFoo = baml.json.from_string<Foo>(`{"a":1,"b":2,"c":3}`) else {
        throw "the extended class does not implement ExtFoo"
    };
    assert.equal(ext_field_names(foo_t), "a,b,c");
    assert.is_true(foo_t.implements(reflect.Type.of<ExtFoo>()));
    assert.equal(foo.a + foo.b, 3)
}

test "extends_implements_the_requires_closure_base_first" {
    let foo_bar_t = reflect.Class.extends<ExtFooBar>("FooBar", { "d": float.type() });
    let ty = foo_bar_t.as_type();
    assert.equal(ext_field_names(foo_bar_t), "a,b,c,d");
    assert.is_true(ty.implements(reflect.Type.of<ExtHasA>()));
    assert.is_true(ty.implements(reflect.Type.of<ExtHasB>()));
    assert.is_true(ty.implements(reflect.Type.of<ExtFooBar>()))
}

test "extends_takes_a_diamond_interface_once" {
    let diamond_t = reflect.Class.extends<ExtDiamond>("Diamond", { "d": int.type() });
    let ty = diamond_t.as_type();
    assert.equal(ext_field_names(diamond_t), "a,left,right,d");
    assert.is_true(ty.implements(reflect.Type.of<ExtHasA>()));
    assert.is_true(ty.implements(reflect.Type.of<ExtLeft>()));
    assert.is_true(ty.implements(reflect.Type.of<ExtRight>()));
    assert.is_true(ty.implements(reflect.Type.of<ExtDiamond>()))
}

test "extends_takes_kind_views_as_field_types" {
    let item_t = reflect.Class.new("ExtItem", { "x": int.type() });
    let list_t = reflect.Class.extends<ExtFoo>("ItemList", { "first": item_t, "items": item_t.array() });
    assert.equal(ext_field_names(list_t), "a,b,first,items");
    assert.is_true(list_t.fields()[2].type == item_t.as_type());
    assert.is_true(list_t.fields()[3].type == item_t.array().as_type())
}

test "extends_keeps_interface_field_docs" {
    let foo_bar_t = reflect.Class.extends<ExtFooBar>(
        "FooBar",
        { "d": int.type().meta(description = "The d field.") },
    );
    let source = foo_bar_t.to_baml();
    assert.contains(source, "  /// The a field.\n  a int\n");
    assert.contains(source, `  d int @description("The d field.")`)
}

test "extends_realizes_generic_requirements" {
    let box_t = reflect.Class.extends<ExtNamedBox<int>>("NamedBox", {});
    let ty = box_t.as_type();
    assert.equal(ext_field_names(box_t), "value,name");
    assert.is_true(box_t.fields()[0].type == int.type());
    assert.is_true(ty.implements(reflect.Type.of<ExtBox<int>>()));
    assert.is_true(ty.implements(reflect.Type.of<ExtNamedBox<int>>()))
}

test "extends_realizes_associated_field_types" {
    let keyed_t = reflect.Class.extends<ExtKeyed<Key = string>>("Keyed", {});
    assert.equal(ext_field_names(keyed_t), "key");
    assert.is_true(keyed_t.fields()[0].type == string.type());
    assert.is_true(keyed_t.implements(reflect.Type.of<ExtKeyed<Key = string>>()))
}

test "extends_merges_a_field_two_interfaces_declare_at_one_type" {
    let shares_t = reflect.Class.extends<ExtSharesA>("SharesA", {});
    let ty = shares_t.as_type();
    assert.equal(ext_field_names(shares_t), "a");
    assert.is_true(ty.implements(reflect.Type.of<ExtHasA>()));
    assert.is_true(ty.implements(reflect.Type.of<ExtAlsoA>()));
    assert.is_true(ty.implements(reflect.Type.of<ExtSharesA>()))
}

test "extends_adopts_interface_default_methods" {
    let greeter_t = reflect.Class.extends<ExtGreeter>("Greeter", {});
    let type Greeter = greeter_t.as_type();
    let greeter: ExtGreeter = baml.json.from_string<Greeter>(`{"name":"Ada"}`) else {
        throw "the extended class does not implement ExtGreeter"
    };
    assert.equal(greeter.greet(), "hello, Ada")
}

test "extends_rejects_a_given_field_an_interface_declares" {
    let result = reflect.Class.extends<ExtFooBar>("FooBar", { "b": string.type() }) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message
    };
    if (result is string) {
        assert.contains(result, "duplicate field `FooBar.b`");
        assert.contains(result, "ExtHasB")
    } else {
        throw "a duplicate field did not throw"
    }
}

test "extends_rejects_a_field_two_interfaces_declare_at_different_types" {
    let result = reflect.Class.extends<ExtConflict>("Conflict", {}) catch (e) {
        reflect.errors.CompilationError => `${e.diagnostics.length()}|${e.diagnostics[0].message}`
    };
    if (result is string) {
        assert.is_true(result.starts_with("1|"));
        assert.contains(result, "both declare field `a`, with different types `int` and `string`")
    } else {
        throw "a conflicting field did not throw"
    }
}

test "extends_rejects_an_interface_with_a_bodyless_method" {
    let direct = reflect.Class.extends<ExtDescribed>("Described", {}) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message
    };
    let required = reflect.Class.extends<ExtRequiresDescribed>("Described", {}) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message
    };
    if (direct is string) {
        assert.contains(direct, "cannot be witnessed structurally");
        assert.contains(direct, "`describe`")
    } else {
        throw "a bodyless method did not throw"
    };
    if (required is string) {
        assert.contains(required, "cannot be witnessed structurally");
        assert.contains(required, "`describe`")
    } else {
        throw "a required interface's bodyless method did not throw"
    }
}

test "extends_rejects_a_type_that_is_not_an_interface" {
    let primitive = reflect.Class.extends<int>("NotExtended", {}) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message
    };
    let class_arg = reflect.Class.extends<ExtNotAnInterface>("NotExtended", {}) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message
    };
    if (primitive is string) {
        assert.equal(primitive, "reflect.Class.extends expects an interface type, got `int`")
    } else {
        throw "a primitive type argument did not throw"
    };
    if (class_arg is string) {
        assert.contains(class_arg, "expects an interface type")
    } else {
        throw "a class type argument did not throw"
    }
}
