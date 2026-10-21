// An alias is the very type it names, so it qualifies a static exactly as the
// class does — and an alias that PINS class arguments (`type IntBox =
// Box<int>`) supplies them to the callee: `IntBox.of(7)` runs `Box.of` with
// `T = int`. Each method below reads its type parameters back through
// reflection, so a frame seeded without the pinned arguments cannot pass.
//
// BUG: a generic static used as a VALUE is not covered, with or without an
// alias: `let make: (int) -> Box<int> throws never = Box.of; make(7)`
// type-checks and then fails at run time ("template references frame type-arg
// slot 0 but the frame has 0 type args") — nothing carries the solved frame to
// a reference that is not a call.

interface Buildable {
    function build(n: int) -> Self throws never
}

class Box<T> {
    item T

    function of(item: T) -> Box<T> throws never {
        Box { item: item }
    }

    function kind() -> string throws never {
        reflect.Type.of<T>().to_string()
    }

    function pair<U>(item: T, other: U) -> string throws never {
        reflect.Type.of<T>().to_string() + "+" + reflect.Type.of<U>().to_string()
    }

    function tagged<U>(self, other: U) -> string throws never {
        reflect.Type.of<T>().to_string() + "+" + reflect.Type.of<U>().to_string()
    }
}

implements Buildable for Box<int> {
    function build(n: int) -> Box<int> throws never {
        Box { item: n }
    }
}

type IntBox = Box<int>
type IntBoxAlias = IntBox
type Ints = int[]

test "alias_pins_the_class_arguments_of_a_static" {
    assert.equal(IntBox.of(7).item, 7);
    assert.equal(IntBox.kind(), "int")
}

test "alias_pins_the_prefix_and_the_call_supplies_the_methods_own_arguments" {
    assert.equal(IntBox.pair<string>(1, "x"), "int+string");
    assert.equal(IntBox.pair(1, true), "int+bool")
}

test "alias_of_a_pinning_alias_pins_the_same_arguments" {
    assert.equal(IntBoxAlias.kind(), "int");
    assert.equal(IntBoxAlias.of(3).item, 3)
}

test "instance_method_called_statically_through_a_pinning_alias" {
    let b = Box { item: 5 };
    assert.equal(IntBox.tagged(b, "x"), "int+string")
}

test "implements_block_static_through_a_pinning_alias" {
    assert.equal(IntBox.build(2).item, 2)
}

test "alias_of_a_builtin_carrier_qualifies_its_statics" {
    assert.equal(Ints.filled(2, 9).length(), 2)
}

test "class_qualified_statics_are_unchanged" {
    assert.equal(Box<int>.kind(), "int");
    assert.equal(Box.of(8).item, 8);
    assert.equal(Box.tagged(Box { item: 5 }, "x"), "int+string")
}
