// The `Self` frame slot: interface-owned default-method bodies carry the
// receiver's concrete type at frame slot 0 (layout
// `[Self ++ interface generics ++ associated types ++ fn generics]`), so
// body-position `Self` — `is` tests, match arms, `reflect.type_of<Self>()` —
// resolves to the *implementor* type at runtime through every seeding path:
// the impl rule's realized frame (virtual/existential dispatch and plain
// concrete-receiver calls), bound methods (frame materialized at bind time),
// closures created inside a default (positional frame capture), and the
// static `default.<method>()` bypass.
//
// Test bodies live in top-level helper functions (test-block local-boxing VM
// bug — locals holding instances misbehave inside test blocks).

interface Kinded {
    /// `is Self` over the top type: a real runtime test of the receiver's
    /// concrete type. (A rigid var vs an `unknown` scrutinee is *possible* —
    /// bounds must not refute a top-type member.)
    function same_kind(self, other: unknown) -> bool throws never {
        other is Self
    }

    /// `is Self` over an interface-existential scrutinee: membership, not
    /// equality — the arm is possible for any implementor value.
    function same_kind_of(self, other: Kinded) -> bool throws never {
        other is Self
    }

    /// Match-arm dispatch on `Self` (possible-but-not-covering: `_` required).
    function match_kind(self, other: unknown) -> string throws never {
        match (other) {
            let s: Self => "self",
            _ => "other",
        }
    }

    /// Reflect the concrete implementor type.
    function kind_name(self) -> string throws never {
        reflect.type_of<Self>().to_string()
    }

    /// A closure created in the default body captures the frame — including
    /// the `Self` slot — positionally.
    function kind_namer(self) -> (() -> string throws never) throws never {
        return () -> string { reflect.type_of<Self>().to_string() };
    }
}

class Dog {
    implements Kinded {}
}

class Cat {
    implements Kinded {}
}

// Generic implementor: slot 0 realizes to the receiver's *structural* type
// (`int[]`), and two instantiations of the same blanket impl stay distinct.
implements<T> Kinded for T[] {}

// ─── Concrete receivers ───────────────────────────────────────────────────────

function dog_same_kind_dog_fn() -> bool {
    let d = Dog {};
    d.same_kind(Dog {})
}

test "self_is_true_for_same_concrete_type" {
    assert.is_true(dog_same_kind_dog_fn())
}

function dog_same_kind_cat_fn() -> bool {
    let d = Dog {};
    d.same_kind(Cat {})
}

test "self_is_false_for_other_implementor" {
    assert.is_true(!dog_same_kind_cat_fn())
}

function dog_same_kind_int_fn() -> bool {
    let d = Dog {};
    d.same_kind(42)
}

test "self_is_false_for_non_implementor" {
    assert.is_true(!dog_same_kind_int_fn())
}

function dog_same_kind_of_dog_fn() -> bool {
    let d = Dog {};
    let other: Kinded = Dog {};
    d.same_kind_of(other)
}

test "self_is_true_over_existential_scrutinee" {
    assert.is_true(dog_same_kind_of_dog_fn())
}

function dog_same_kind_of_cat_fn() -> bool {
    let d = Dog {};
    let other: Kinded = Cat {};
    d.same_kind_of(other)
}

test "self_is_false_over_existential_scrutinee" {
    assert.is_true(!dog_same_kind_of_cat_fn())
}

function dog_match_kind_dog_fn() -> string {
    let d = Dog {};
    d.match_kind(Dog {})
}

test "self_match_arm_takes_same_concrete_type" {
    assert.equal(dog_match_kind_dog_fn(), "self")
}

function dog_match_kind_cat_fn() -> string {
    let d = Dog {};
    d.match_kind(Cat {})
}

test "self_match_arm_rejects_other_implementor" {
    assert.equal(dog_match_kind_cat_fn(), "other")
}

function dog_kind_name_fn() -> string {
    let d = Dog {};
    d.kind_name()
}

test "self_reflects_concrete_implementor" {
    assert.equal(dog_kind_name_fn(), "self_frame_slot.Dog")
}

// ─── Generic (blanket) implementor: structural Self at slot 0 ─────────────────

function int_list_same_kind_int_list_fn() -> bool {
    let xs: int[] = [1, 2, 3];
    let more: int[] = [4];
    xs.same_kind(more)
}

test "blanket_self_is_true_for_same_instantiation" {
    assert.is_true(int_list_same_kind_int_list_fn())
}

function int_list_same_kind_string_list_fn() -> bool {
    let xs: int[] = [1, 2, 3];
    let ys: string[] = ["a"];
    xs.same_kind(ys)
}

test "blanket_self_is_false_for_other_instantiation" {
    assert.is_true(!int_list_same_kind_string_list_fn())
}

function int_list_kind_name_fn() -> string {
    let xs: int[] = [1, 2, 3];
    xs.kind_name()
}

test "blanket_self_reflects_structural_receiver" {
    assert.equal(int_list_kind_name_fn(), "int[]")
}

// ─── Existential dispatch, bound methods, closures ────────────────────────────

function existential_kind_name_fn() -> string {
    let k: Kinded = Dog {};
    k.kind_name()
}

test "existential_dispatch_seeds_self_slot" {
    assert.equal(existential_kind_name_fn(), "self_frame_slot.Dog")
}

function bound_method_kind_name_fn() -> string {
    let d = Dog {};
    let bm = d.kind_name;
    bm()
}

test "bound_method_materializes_self_slot" {
    assert.equal(bound_method_kind_name_fn(), "self_frame_slot.Dog")
}

function closure_kind_name_fn() -> string {
    let c = Cat {};
    let f = c.kind_namer();
    f()
}

test "default_body_closure_captures_self_slot" {
    assert.equal(closure_kind_name_fn(), "self_frame_slot.Cat")
}

function blanket_closure_kind_name_fn() -> string {
    let xs: string[] = ["a"];
    let f = xs.kind_namer();
    f()
}

test "blanket_closure_captures_structural_self" {
    assert.equal(blanket_closure_kind_name_fn(), "string[]")
}

// ─── Generic interface + associated type + `default.<method>()` bypass ────────

interface Store<T> {
    type Key;

    /// Reads all three frame regions: `Self` (slot 0), the interface generic,
    /// and the associated type.
    function describe(self) -> string throws never {
        reflect.type_of<Self>().to_string()
            + "/" + reflect.type_of<T>().to_string()
            + "/" + reflect.type_of<Self.Key>().to_string()
    }
}

class IntShelf {
    implements Store<int> {
        type Key = string;

        /// Override that explicitly bypasses to the default: the static
        /// `default.<method>()` frame seeding (`[Self ++ T ++ Key]`).
        function describe(self) -> string throws never {
            "override->" + default.describe()
        }
    }
}

class FloatBin {
    implements Store<float> {
        type Key = bool;
    }
}

function shelf_describe_fn() -> string {
    let shelf = IntShelf {};
    shelf.describe()
}

test "default_bypass_seeds_self_generic_and_assoc" {
    assert.equal(shelf_describe_fn(), "override->self_frame_slot.IntShelf/int/string")
}

function bin_describe_fn() -> string {
    let bin = FloatBin {};
    bin.describe()
}

test "inherited_default_frame_carries_self_generic_and_assoc" {
    assert.equal(bin_describe_fn(), "self_frame_slot.FloatBin/float/bool")
}

// ─── Implements-block bodies: `Self` lowers STATICALLY to the `for` target ────
//
// In an implements block (in-body or free), `Self` is logically a type
// variable, but its sole realization is the block's subject — so TIR
// substitutes it statically (no frame slot involved): a concrete `for` type,
// the impl's generic pattern (`T[]` over the impl frame), or the class at its
// own params.

interface Marked {
    function mark(self) -> string throws never
    function is_self(self, other: unknown) -> bool throws never
    function type_name(self) -> string throws never
}

implements Marked for int {
    function mark(self) -> string throws never {
        "int"
    }
    function is_self(self, other: unknown) -> bool throws never {
        other is Self
    }
    function type_name(self) -> string throws never {
        reflect.type_of<Self>().to_string()
    }
}

implements<T> Marked for T[] {
    function mark(self) -> string throws never {
        "list"
    }
    function is_self(self, other: unknown) -> bool throws never {
        other is Self
    }
    function type_name(self) -> string throws never {
        reflect.type_of<Self>().to_string()
    }
}

class Croc {
    implements Marked {
        function mark(self) -> string throws never {
            "croc"
        }
        function is_self(self, other: unknown) -> bool throws never {
            other is Self
        }
        function type_name(self) -> string throws never {
            reflect.type_of<Self>().to_string()
        }
    }
}

class Crate<T> {
    v: T,
    implements Marked {
        function mark(self) -> string throws never {
            "crate"
        }
        function is_self(self, other: unknown) -> bool throws never {
            other is Self
        }
        function type_name(self) -> string throws never {
            reflect.type_of<Self>().to_string()
        }
    }
}

function free_impl_concrete_self_fn() -> bool {
    let n = 5;
    n.is_self(7) && !n.is_self("s")
}

test "free_impl_self_is_static_concrete_type" {
    assert.is_true(free_impl_concrete_self_fn())
}

function free_impl_concrete_self_name_fn() -> string {
    let n = 5;
    n.type_name()
}

test "free_impl_self_reflects_static_concrete_type" {
    assert.equal(free_impl_concrete_self_name_fn(), "int")
}

function free_impl_generic_self_fn() -> bool {
    let xs: int[] = [1];
    let ys: string[] = ["a"];
    let more: int[] = [2, 3];
    xs.is_self(more) && !xs.is_self(ys)
}

test "free_impl_self_is_static_generic_pattern" {
    assert.is_true(free_impl_generic_self_fn())
}

function free_impl_generic_self_name_fn() -> string {
    let xs: int[] = [1];
    xs.type_name()
}

test "free_impl_self_reflects_static_generic_pattern" {
    assert.equal(free_impl_generic_self_name_fn(), "int[]")
}

function in_body_self_fn() -> bool {
    let c = Croc {};
    c.is_self(Croc {}) && !c.is_self(1)
}

test "in_body_impl_self_is_static_class" {
    assert.is_true(in_body_self_fn())
}

function in_body_generic_self_fn() -> bool {
    let ci: Crate<int> = Crate { v: 1 };
    let cs: Crate<string> = Crate { v: "s" };
    ci.is_self(Crate { v: 2 }) && !ci.is_self(cs) && !ci.is_self(1)
}

test "in_body_impl_self_is_static_class_at_own_params" {
    assert.is_true(in_body_generic_self_fn())
}

function in_body_generic_self_name_fn() -> string {
    let ci: Crate<int> = Crate { v: 1 };
    ci.type_name()
}

test "in_body_impl_self_reflects_receiver_instantiation" {
    assert.equal(in_body_generic_self_name_fn(), "self_frame_slot.Crate<int>")
}
