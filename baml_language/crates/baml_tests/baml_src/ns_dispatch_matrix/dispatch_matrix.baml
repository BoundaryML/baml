// THE DISPATCH MATRIX. One row per way an interface method can be reached,
// end to end, so a change to any road breaks loudly here first. The frame
// convention every row exercises is `[owner ++ own generics]`, with exactly
// two owner shapes: the impl's generics for a PROVIDED method, and
// `[Self, interface args..]` for an ADOPTED default (associated types are
// never frame slots).
//
//   row  call form                        road / frame source
//   ---  -------------------------------  ---------------------------------
//   1    call, provided, frame-free       virtual; rule frame at dispatch
//   2    call, provided, generic impl     virtual; rule frame at dispatch
//   3    call, adopted default            virtual; rule frame at dispatch
//   4    call, provided, primitive        virtual; rule frame at dispatch
//   5    interface-typed receiver         virtual; rule frame at dispatch
//   6    virtual bound reference          rule frame curried at binding
//   7    concrete-receiver bound ref      rule frame curried at binding
//   8    `default.<method>()` bypass      direct; frame built statically
//   9    same-interface union             virtual on the shared interface
//   10   type-qualified (`C.member`)      MakeVirtualFunction; Self passed
//   11   written qualifier `(C as I).m`   same road, qualifier written
//   12   interface-qualified (`I.m(x)`)   Self inferred from the argument;
//                                         dynamic for an existential arg
//                                         (object-safety rules)
//   13   value-position shim              rule frame realized by the shim
//
// Rows 1-5 all dispatch open-world (every interface-mediated call is a
// virtual call): a concrete receiver's TIR resolution still CARRIES the
// owner frame — the residual direct road and the metadata consumers read
// it — but the emitted call keys on the declaring interface and the RULE
// realizes the frame from the runtime receiver. What each row pins is
// that the realized frame is right for its rule kind. The compare /
// to_json shims share row 12's road (pinned in ns_comparable_sort /
// ns_to_json_interface).
//
// Test bodies live in top-level helper functions (test-block local-boxing VM
// bug — locals holding instances misbehave inside test blocks).

interface Speaker {
    function speak(self) -> string throws never {
        "default"
    }
}

class Loud {
    implements Speaker {
        function speak(self) -> string throws never {
            "loud"
        }
    }
}

class Quiet {
    implements Speaker {}
}

implements Speaker for int {
    function speak(self) -> string throws never {
        "int"
    }
}

class Pack<T> {
    leader T
}

implements<T> Speaker for Pack<T> {
    function speak(self) -> string throws never {
        "pack of " + reflect.Type.of<T>().to_string()
    }
}

class Wrapping {
    implements Speaker {
        function speak(self) -> string throws never {
            "wrapped " + default.speak()
        }
    }
}

// Row 1: static call, provided method, frame-free impl.
function row1_static_provided() -> string throws never {
    let l = Loud {};
    l.speak()
}

// Row 2: provided method on a generic impl — the rule realizes the impl's
// bindings (`T = int`) for the receiver, observed by reflection.
function row2_static_generic_bindings() -> string throws never {
    let p = Pack { leader: 1 };
    p.speak()
}

// Row 3: static call reaching an adopted default with `[Self = Quiet]`.
function row3_static_adopted_default() -> string throws never {
    let q = Quiet {};
    q.speak()
}

// Row 4: static call, provided method whose for-target is a primitive.
function row4_static_primitive_target() -> string throws never {
    let n = 3;
    n.speak()
}

// Row 5: open-world virtual dispatch through an interface-typed value —
// once per rule kind, since each realizes a different frame shape.
function through_interface(s: Speaker) -> string throws never {
    s.speak()
}

function row5_virtual() -> string throws never {
    through_interface(Loud {})
        + "|" + through_interface(Quiet {})
        + "|" + through_interface(3)
        + "|" + through_interface(Pack { leader: true })
}

// Row 6: a bound reference taken from an interface-typed value dispatches
// like the virtual call it defers.
function row6_virtual_bound_ref(s: Speaker) -> string throws never {
    let f = s.speak;
    f()
}

// Row 7: a bound reference taken from a concrete receiver curries the
// rule's realized frame — provided and adopted alike.
function row7_static_bound_refs() -> string throws never {
    let l = Loud {};
    let provided = l.speak;
    let q = Quiet {};
    let adopted = q.speak;
    provided() + "|" + adopted()
}

// Row 8: `default.<method>()` delegates to the interface's default body
// with a statically-built `[Self, iface args..]` frame (never re-entering
// the impl's own method).
function row8_default_bypass() -> string throws never {
    let w = Wrapping {};
    w.speak()
}

// Row 9: a same-interface union dispatches open-world on the shared
// interface — the members' interface methods are the ONLY member surface a
// union receiver has.
function pick_speaker(loud: bool) -> Loud | Quiet throws never {
    if (loud) {
        Loud {}
    } else {
        Quiet {}
    }
}

function row9_union() -> string throws never {
    pick_speaker(true).speak() + "|" + pick_speaker(false).speak()
}

// Rows 10/11: a SELF-LESS interface member reached through the type — with
// the qualifying interface inferred (bare `C.member`) and written.
interface Counter {
    function start() -> int throws never {
        0
    }
}

class Tally {
    implements Counter {
        function start() -> int throws never {
            10
        }
    }
}

class FreshCount {
    implements Counter {}
}

function row10_type_qualified() -> int throws never {
    Tally.start() + FreshCount.start()
}

function row11_written_qualifier() -> int throws never {
    (Tally as Counter).start()
}

// Row 12: the triad's third spelling — the INTERFACE qualifies and `Self`
// is inferred from the receiver argument (UFCS): statically for a concrete
// argument, dynamically for an existential one (the method is object-safe:
// one `Self`, in receiver position).
function row12_interface_qualified() -> string throws never {
    let l = Loud {};
    Speaker.speak(l)
}

function row12_interface_qualified_dynamic(s: Speaker) -> string throws never {
    Speaker.speak(s)
}

// Row 13: the value-position shim road — interpolation renders through the
// impl rule's provided `to_string`, exactly as `dispatch_op` reads operators.
class Fancy {
    implements baml.ToString {
        function to_string(self) -> string throws never {
            "<fancy>"
        }
    }
}

function row13_shim_to_string() -> string throws never {
    let f = Fancy {};
    `${f}`
}

test "row1 static call reaches a provided method" {
    assert.equal(row1_static_provided(), "loud")
}

test "row2 static call carries a generic impl's bindings" {
    assert.equal(row2_static_generic_bindings(), "pack of int")
}

test "row3 static call reaches an adopted default" {
    assert.equal(row3_static_adopted_default(), "default")
}

test "row4 static call reaches a primitive-target impl" {
    assert.equal(row4_static_primitive_target(), "int")
}

test "row5 virtual dispatch realizes every rule kind's frame" {
    assert.equal(row5_virtual(), "loud|default|int|pack of bool")
}

test "row6 virtual bound reference dispatches like the call" {
    assert.equal(row6_virtual_bound_ref(Loud {}), "loud")
    assert.equal(row6_virtual_bound_ref(Quiet {}), "default")
}

test "row7 static bound references curry the realized frame" {
    assert.equal(row7_static_bound_refs(), "loud|default")
}

test "row8 default bypass reaches the interface body" {
    assert.equal(row8_default_bypass(), "wrapped default")
}

test "row9 same-interface union dispatches on the shared interface" {
    assert.equal(row9_union(), "loud|default")
}

test "row10 bare type-qualified members resolve through the impl" {
    assert.equal(row10_type_qualified(), 10)
}

test "row11 written qualifier resolves the same member" {
    assert.equal(row11_written_qualifier(), 10)
}

test "row12 interface-qualified calls infer Self from the argument" {
    assert.equal(row12_interface_qualified(), "loud")
    assert.equal(row12_interface_qualified_dynamic(Quiet {}), "default")
    assert.equal(row12_interface_qualified_dynamic(3), "int")
}

test "row13 interpolation renders through the provided to_string" {
    assert.equal(row13_shim_to_string(), "<fancy>")
}
