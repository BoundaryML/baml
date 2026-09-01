// Fully-qualified item references — the value-namespace half of the
// `(Self type, interface, item)` triple. The spellings differ only in which
// halves are written; past inference they are one resolution:
//
//   (Base as I).item     both halves written
//   I.item               `Self` inferred (from arguments or expectation)
//   Base.item            interface inferred (the class-member road)
//
// A method WITH a `self` receiver dispatches on the written first argument
// (UFCS); a method WITHOUT one dispatches on the `Self` TYPE — the only
// form that works when no receiver exists, including a typevar `Self`
// realized from the caller's frame at runtime.

interface Sizer {
    function pick(self, other: Self) -> Self throws never
    function grow(self) -> Self throws never { self.pick(self) }
}

// The same method name under two interfaces: the bare spelling is ambiguous
// (E0121), and the qualifier is the only road to each impl.
interface Alpha { function tag(self, other: Self) -> int throws never }
interface Beta { function tag(self, other: Self) -> int throws never }

class Twin {
    n: int

    implements Alpha {
        function tag(self, other: Self) -> int throws never {
            self.n
        }
    }

    implements Beta {
        function tag(self, other: Self) -> int throws never {
            other.n
        }
    }
}

test "qualifier_selects_between_same_named_methods" {
    let x = Twin { n: 1 }
    let y = Twin { n: 2 }
    assert.equal((Twin as Alpha).tag(x, y), 1)
    assert.equal((Twin as Beta).tag(x, y), 2)
}

// Either half of the qualifier may be redundantly parenthesized. One
// lookahead decides the projection shape for both namespaces, so the value
// spellings here and the type spellings in `interfaces_associated_types`
// accept exactly the same bases — the two cannot drift apart.
test "parenthesized_qualifier_halves" {
    let x = Twin { n: 1 }
    let y = Twin { n: 2 }
    assert.equal(((Twin) as Alpha).tag(x, y), 1)
    assert.equal((Twin as (Beta)).tag(x, y), 2)
    assert.equal(((Twin) as (Alpha)).tag(x, y), 1)
}

// A generic interface implemented at two instantiations: the qualifier's
// generic arguments select the impl, so they are part of its identity.
interface Convert<T> {
    function conv(self, other: Self) -> T throws never
}

class Poly {
    n: int

    implements Convert<int> {
        function conv(self, other: Self) -> int throws never {
            self.n + other.n
        }
    }

    implements Convert<string> {
        function conv(self, other: Self) -> string throws never {
            "poly"
        }
    }
}

test "qualifier_generics_select_the_instantiation" {
    let a = Poly { n: 3 }
    let b = Poly { n: 4 }
    assert.equal((Poly as Convert<int>).conv(a, b), 7)
    assert.equal((Poly as Convert<string>).conv(a, b), "poly")
}

// Self-less interface statics: `Self` appears only in the return type, so
// there is no receiver to derive it from — it must be written, realized from
// the caller's frame, or solved from the expected type.
interface Buildable {
    function build(seed: int) -> Self throws never
    function describe(self) -> string throws never
}

class Widget {
    w: int

    implements Buildable {
        function build(seed: int) -> Self throws never {
            Widget { w: seed }
        }

        function describe(self) -> string throws never {
            "widget " + self.w.to_string()
        }
    }
}

class Gadget {
    g: int

    implements Buildable {
        function build(seed: int) -> Self throws never {
            Gadget { g: seed * 10 }
        }

        function describe(self) -> string throws never {
            "gadget " + self.g.to_string()
        }
    }
}

test "selfless_static_with_written_self" {
    assert.equal((Widget as Buildable).build(7).describe(), "widget 7")
    assert.equal((Gadget as Buildable).build(7).describe(), "gadget 70")
}

// A typevar `Self` resolves from the caller's realized frame at runtime —
// the case with no receiver value anywhere.
function build_any<T extends Buildable>(seed: int) -> T throws never {
    (T as Buildable).build(seed)
}

test "selfless_static_with_typevar_self" {
    let w: Widget = build_any(3)
    let g: Gadget = build_any(3)
    assert.equal(w.describe(), "widget 3")
    assert.equal(g.describe(), "gadget 30")
}

test "selfless_static_with_inferred_self" {
    let w: Widget = Buildable.build(5)
    assert.equal(w.describe(), "widget 5")
}

// The `I.item(recv, ..)` UFCS spelling for a receiver method: `Self` solves
// from the first argument.
test "ufcs_receiver_method_through_bare_interface" {
    let w = Widget { w: 9 }
    assert.equal(Buildable.describe(w), "widget 9")
}

// A default method reached through the qualifier: the impl provides no
// override, so the interface's body runs with `Self` bound at the site.
class Sized {
    s: int

    implements Sizer {
        function pick(self, other: Self) -> Self throws never {
            if (self.s > other.s) {
                self
            } else {
                other
            }
        }
    }
}

test "qualified_default_method" {
    let a = Sized { s: 1 }
    let b = Sized { s: 5 }
    assert.equal((Sized as Sizer).pick(a, b).s, 5)
    assert.equal(Sizer.pick(a, b).s, 5)
}

// The value form: a qualified reference is a first-class function whose
// frame is curried at resolution.
test "qualified_reference_as_value" {
    let make = (Gadget as Buildable).build
    assert.equal(make(4).describe(), "gadget 40")
    let pick = (Sized as Sizer).pick
    assert.equal(pick(Sized { s: 2 }, Sized { s: 8 }).s, 8)
}

// ─── Audit regressions ───────────────────────────────────────────────────────
// Locked-in repros from the adversarial soundness audit.

// A `self`-less method is reached through the TYPE, never through a value: a
// value carries nothing the call needs, and reading one as a receiver is what
// let it be smuggled into the first real parameter. The receiver spellings are
// compile errors (see `interfaces.rs`); these are the roads that remain.
test "selfless_static_is_reached_through_the_type" {
    assert.equal(Widget.build(5).describe(), "widget 5")
    assert.equal((Widget as Buildable).build(9).describe(), "widget 9")
    let m = (Widget as Buildable).build
    assert.equal(m(3).describe(), "widget 3")
}

// The bare `I.item` VALUE form with `Self` solved retroactively from a later
// call, and pinned by annotation — both resolve the callable from the
// recorded frame (previously an emit panic).
test "bare_interface_method_value_forms" {
    let f = Sizer.pick
    assert.equal(f(Sized { s: 2 }, Sized { s: 6 }).s, 6)
    let g: (Sized, Sized) -> Sized throws never = Sizer.pick
    assert.equal(g(Sized { s: 4 }, Sized { s: 3 }).s, 4)
}

// A bare default-method value must dispatch through the impl, never call the
// interface's default body directly — an override would otherwise be skipped.
interface Sayer {
    function say(self) -> string throws never {
        "default"
    }
}
class Quiet {
    implements Sayer {}
}
class Loud {
    implements Sayer {
        function say(self) -> string throws never {
            "LOUD"
        }
    }
}
test "bare_default_method_value_respects_overrides" {
    let s = Sayer.say
    assert.equal(s(Quiet {}), "default")
    let l: (Loud) -> string throws never = Sayer.say
    assert.equal(l(Loud {}), "LOUD")
}

// An alias of an interface works as the qualifier, transparently.
type BuildableAlias = Buildable
test "alias_of_interface_as_qualifier" {
    assert.equal((Widget as BuildableAlias).build(2).describe(), "widget 2")
}

// Associated types need not be written in the qualifier: they are uniquely
// determined once `Self` is known.
interface Keyed {
    type Key
    function key(self) -> Self.Key throws never
}
class Row {
    id: int
    implements Keyed {
        type Key = int
        function key(self) -> Self.Key throws never {
            self.id
        }
    }
}
test "qualifier_without_written_associated_types" {
    assert.equal((Row as Keyed).key(Row { id: 7 }), 7)
    let k = (Row as Keyed).key
    assert.equal(k(Row { id: 9 }), 9)
}

// A GENERIC interface's default method as a first-class value through the
// qualifier: the written `Label<int>` pins the interface instantiation as
// well as `Self`, so the value's whole frame is complete at the reference
// site and it runs the default body under that instantiation.
interface Label<T> {
    function label(self) -> string throws never {
        "ok"
    }
}
class Crate {
    implements Label<int> {}
}
test "generic_interface_default_method_reference" {
    let label = (Crate as Label<int>).label
    assert.equal(label(Crate {}), "ok")
}

// ─── Review regressions ──────────────────────────────────────────────────────

// A GENERIC class as `Self`: the callee frame is the receiver's class
// arguments, so every spelling must carry them — a bare function constant
// would run the impl body with an empty frame.
class Bin<T> {
    label: string

    implements Buildable {
        function build(seed: int) -> Self throws never {
            Bin { label: "bin " + seed.to_string() }
        }

        function describe(self) -> string throws never {
            self.label
        }
    }
}

test "selfless_static_on_generic_class_through_every_spelling" {
    assert.equal((Bin<int> as Buildable).build(1).describe(), "bin 1")
    assert.equal(Bin<int>.build(2).describe(), "bin 2")
    let m = (Bin<int> as Buildable).build
    assert.equal(m(3).describe(), "bin 3")
}

// A written qualifier is a CLAIM the environment must prove. A rigid type
// variable matches only through an impl's own parameter — a blanket rule or
// a bound — never by collapsing to one ground instantiation.
interface Marked<V> {
    function mark(self) -> V throws never
}

class Dual {
    implements Marked<int> {
        function mark(self) -> int throws never {
            1
        }
    }

    implements Marked<string> {
        function mark(self) -> string throws never {
            "two"
        }
    }
}

// The bound supplies the proof, and `V` rides the frame: each instantiation
// dispatches to its own impl rather than to whichever was declared first.
function marked_via<V, T extends Marked<V>>(t: T) -> V throws never {
    (T as Marked<V>).mark(t)
}

test "symbolic_qualifier_dispatches_per_instantiation" {
    let i: int = marked_via(Dual {})
    let s: string = marked_via(Dual {})
    assert.equal(i, 1)
    assert.equal(s, "two")
}

// Runtime type arguments (`unreflect`) flow as ordinary operands through the
// qualified spellings, exactly as static ones do.
interface Stamper {
    function stamp<U>(self, u: U) -> string throws never
    function emboss<U>(u: U) -> string throws never
}

class Press {
    implements Stamper {
        function stamp<U>(self, u: U) -> string throws never {
            "stamp " + reflect.Type.of<U>().to_string()
        }

        function emboss<U>(u: U) -> string throws never {
            "emboss " + reflect.Type.of<U>().to_string()
        }
    }
}

test "runtime_type_arguments_flow_through_qualified_spellings" {
    let p = Press {}
    let t = reflect.Type.of<int>()
    assert.equal(Stamper.stamp<unreflect(t)>(p, 5), "stamp int")
    assert.equal((Press as Stamper).stamp<unreflect(t)>(p, 5), "stamp int")
    assert.equal((Press as Stamper).emboss<unreflect(t)>(5), "emboss int")
    assert.equal((Press as Stamper).emboss<string>("x"), "emboss string")
}

// `as` is a contextual keyword and a legal binding name: a parenthesized
// expression using it must not be read as a qualified projection.
test "as_remains_usable_as_an_identifier" {
    let as = 5
    assert.equal((as).to_string(), "5")
    assert.equal((as + 1).to_string(), "6")
}

// An interface method may be named with a contextual keyword; every access
// spelling must reach it, the qualified one included.
interface Keyworded {
    function implements(self) -> int throws never
    function extends(self) -> int throws never
}
class Keyed2 {
    implements Keyworded {
        function implements(self) -> int throws never {
            1
        }

        function extends(self) -> int throws never {
            2
        }
    }
}
test "keyword_named_members_are_reachable_through_the_qualifier" {
    let k = Keyed2 {}
    assert.equal(k.implements(), 1)
    assert.equal((Keyed2 as Keyworded).implements(k), 1)
    assert.equal((Keyed2 as Keyworded).extends(k), 2)
    assert.equal(Keyworded.extends(k), 2)
}

// A `self`-less member reached through the TYPE carries the whole callee
// frame: the owner's class arguments and the method's own generics. Supplying
// only one half leaves the body's slots unrealizable.
interface Embosser {
    function emboss<U>(u: U) -> string throws never
    function fresh(seed: int) -> Self throws never
    function label(self) -> string throws never
}
class Plate<T> {
    tag: string

    implements Embosser {
        function emboss<U>(u: U) -> string throws never {
            "emboss " + reflect.Type.of<U>().to_string()
        }

        function fresh(seed: int) -> Self throws never {
            Plate { tag: seed.to_string() }
        }

        function label(self) -> string throws never {
            "plate " + self.tag
        }
    }
}

test "selfless_member_through_the_type_carries_the_whole_frame" {
    // Own generics, inferred and written, on a generic-class `Self`.
    assert.equal((Plate<int> as Embosser).emboss("x"), "emboss string")
    assert.equal((Plate<int> as Embosser).emboss<bool>(true), "emboss bool")
    // The class half alone: call and value forms both realize the frame.
    assert.equal((Plate<int> as Embosser).fresh(9).label(), "plate 9")
    let make = (Plate<int> as Embosser).fresh
    assert.equal(make(4).label(), "plate 4")
    // A stored generic method value must realize its own generic parameter at
    // the binding; unlike a direct call, a later call cannot specialize it.
    // Qualified interface projections cannot carry standalone `<...>`, so the
    // concrete function annotation is the available spelling here.
    let stamp: (int) -> string throws never = (Plate<int> as Embosser).emboss
    assert.equal(stamp(7), "emboss int")
}
