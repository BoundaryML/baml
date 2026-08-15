// End-to-end tests for the generic-type-arg dispatch fix.
//
// A generic class whose `<T>` is *inferred* (rather than written explicitly)
// must carry the correct runtime `class_type_args`, and a method dispatched
// through a multi-implementor interface must receive the resolved
// class/interface type args in its frame. Without this, a method body that
// reads its enclosing `T` at runtime — `reflect.type_of<T>()`, or constructing
// `Other<T>{}` — resolves `T` to `unknown`; the new instance gets empty
// `class_type_args`; and the runtime `IsType` dispatch guard falls through to
// the WRONG implementor (a silent wrong answer, or a field-access panic when
// the sibling has a different field layout).
//
// Fixed in `baml_compiler2_mir::lower`: the interface-dispatch switch now seeds
// the callee frame's `type_args` per candidate — a class-owned method gets the
// implementor's class args (statically when the guard pins them, otherwise from
// the matched runtime instance via a `BoundMethod`, which covers `Any`/partial
// guards); an inherited default gets the interface's args. `default.<method>()`
// forwarding seeds the same way, and `enclosing_generic_params` includes
// interface params for default methods.
//
// NOTE: test bodies live in top-level helper functions — locals holding class
// instances misbehave when compared/passed inside `test { ... }` blocks (the
// same VM local-boxing caveat documented in `ns_class_type_args_at_runtime`).

// ═══════════════════════════════════════════════════════════════════════════
// Showcase: a lazy cursor protocol — the exact shape `baml.iter` needs.
//
// `Cursor<T>` is a generic interface with a default `take_one(self)` that builds
// a *new* generic adapter (`SingleCursor<T>`) from the interface's element type
// `T`. The two leaf implementors have different field layouts, so a mis-dispatch
// doesn't merely return a wrong value — it reads the wrong field and panics.
//
// The whole pipeline runs through the `Cursor<int>` interface, and the `<int>`
// is never written after the first annotation: `ArrayCursor.of([...])` infers
// it, and the default method must propagate it into the `SingleCursor<int>` it
// constructs. Correctness depends entirely on the inferred `<int>` reaching
// each dispatched frame.
// ═══════════════════════════════════════════════════════════════════════════

interface Cursor<T> {
  function head(self) -> T throws never

  // Default method, inherited by every implementor. It constructs a generic
  // `SingleCursor<T>` from the interface's own `T`. The resulting instance MUST
  // carry `class_type_args = [T]`; otherwise the `single.head()` dispatch in
  // `take_one_then_head` resolves to `ArrayCursor.head` — which indexes an
  // `arr` field that `SingleCursor` does not have — instead of
  // `SingleCursor.head`.
  function take_one(self) -> Cursor<T> throws never {
    return SingleCursor<T> { value: self.head() }
  }
}

class ArrayCursor<T> {
    arr: T[],
    idx: int,

    // Static constructor: `T` is INFERRED from the argument, never written.
    function of(items: T[]) -> ArrayCursor<T> {
        return ArrayCursor<T> { arr: items, idx: 0 };
    }

    implements Cursor<T> {
        function head(self) -> T throws never {
            return self.arr[0];
        }
    }
}

class SingleCursor<T> {
    value: T,

    implements Cursor<T> {
        function head(self) -> T throws never {
            return self.value;
        }
    }
}

// Build an `ArrayCursor<int>` with an inferred `<int>`, narrow it to a single
// element via the inherited default method, then read the head back — entirely
// through the `Cursor<int>` interface. Exercises inferred static-ctor type args
// AND a default method that builds a generic from the interface's `T`.
function take_one_then_head() -> int throws never {
    let cursor: Cursor<int> = ArrayCursor.of([10, 20, 30]);
    let single: Cursor<int> = cursor.take_one();
    single.head()
}

test "showcase_lazy_cursor_pipeline" {
    assert.equal(take_one_then_head(), 10)
}

// ═══════════════════════════════════════════════════════════════════════════
// Focused regression cases
// ═══════════════════════════════════════════════════════════════════════════

// ─── Inferred static constructor + non-generic interface, two implementors ───

interface Animal {
  function speak(self) -> string throws never
}

class Dog<T> {
    value: T,

    function make(v: T) -> Dog<T> {
        return Dog<T> { value: v };
    }

    implements Animal {
        function speak(self) -> string {
            return "woof";
        }
    }
}

class Cat {
    meow_count: int,

    implements Animal {
        function speak(self) -> string {
            return "meow";
        }
    }
}

// `Dog.make(42)` infers `Dog<int>`; dispatched through `Animal` (which `Cat`
// also implements) it must still pick `Dog`'s method.
function inferred_static_ctor_speak() -> string {
    let d: Animal = Dog.make(42);
    d.speak()
}

test "inferred_static_ctor_dispatches_to_correct_implementor" {
    assert.equal(inferred_static_ctor_speak(), "woof")
}

// ─── Inferred static constructor + generic interface, mismatched shapes ───

interface Bag<T> {
  function first(self) -> T throws never
}

class ArrBag<T> {
    arr: T[],
    idx: int,

    function make(a: T[]) -> ArrBag<T> {
        return ArrBag<T> { arr: a, idx: 0 };
    }

    implements Bag<T> {
        function first(self) -> T throws never {
            return self.arr[0];
        }
    }
}

class OneBag<T> {
    only: T,

    implements Bag<T> {
        function first(self) -> T throws never {
            return self.only;
        }
    }
}

// A mis-dispatch would read `only` off an `ArrBag` (or index `arr` on a
// `OneBag`) — different layouts, so the wrong arm panics rather than silently
// returning a bad value.
function inferred_static_ctor_first() -> int throws never {
    let s: Bag<int> = ArrBag.make([5, 6, 7]);
    s.first()
}

test "inferred_static_ctor_dispatches_through_generic_interface" {
    assert.equal(inferred_static_ctor_first(), 5)
}

// ─── Interface default method can read the interface's type var at runtime ───

interface Named<T> {
  function first(self) -> T throws never
  function tname(self) -> string throws never {
    return reflect.type_of<T>().to_string()
  }
}

class ArrNamed<T> {
    arr: T[],
    idx: int,
    implements Named<T> {
        function first(self) -> T throws never {
            return self.arr[0];
        }
    }
}

class OneNamed<T> {
    only: T,
    implements Named<T> {
        function first(self) -> T throws never {
            return self.only;
        }
    }
}

// `tname` is a default method dispatched through `Named<int>`; it must see
// `T == int`, not `unknown`/`void`.
function default_method_typevar_name() -> string throws never {
    let s: Named<int> = ArrNamed<int> { arr: [5, 6, 7], idx: 0 };
    s.tname()
}

test "interface_default_method_sees_interface_type_var" {
    assert.equal(default_method_typevar_name(), "int")
}

// ─── Class-owned (non-default) method that builds a generic from class `T` ───

interface Wrap<T> {
  function first(self) -> T throws never
}

class SrcWrap<T> {
    arr: T[],
    idx: int,

    // A regular class method (not an interface default) that constructs a
    // generic `Boxed<T>` from the class-level `T`.
    function boxed(self) -> Wrap<T> throws never {
        return Boxed<T> { val: self.arr[0] };
    }

    implements Wrap<T> {
        function first(self) -> T throws never {
            return self.arr[0];
        }
    }
}

class Boxed<T> {
    val: T,
    implements Wrap<T> {
        function first(self) -> T throws never {
            return self.val;
        }
    }
}

function class_method_builds_generic() -> int throws never {
    let s: SrcWrap<int> = SrcWrap<int> { arr: [5, 6, 7], idx: 0 };
    let w: Wrap<int> = s.boxed();
    w.first()
}

test "class_method_builds_generic_via_class_type_var" {
    assert.equal(class_method_builds_generic(), 5)
}

// ─── `default.<method>()` forwards into a generic interface default ───
//
// Calling the interface's default implementation explicitly (the `super`-like
// `default.<method>()` form) from inside an override must seed the default
// body's frame with the interface's type args, so a generic default that reads
// `T` at runtime resolves it instead of seeing `unknown`.

interface Counter<T> {
  function element_type(self) -> string throws never {
    return reflect.type_of<T>().to_string()
  }
  function describe(self) -> string throws never
}

class IntCounter<T> {
    value: T,
    implements Counter<T> {
        // Override `describe` but delegate to the generic default `element_type`.
        function describe(self) -> string {
            return default.element_type();
        }
    }
}

class StrCounter<T> {
    label: T,
    implements Counter<T> {
        function describe(self) -> string {
            return "str";
        }
    }
}

function default_forward_reads_type_var() -> string {
    let c: Counter<int> = IntCounter<int> { value: 7 };
    c.describe()
}

test "default_method_forward_reads_interface_type_var" {
    assert.equal(default_forward_reads_type_var(), "int")
}

// ─── Generic class behind a NON-generic interface reads its class `T` ───
//
// The interface request pins no type args (it has none), so the dispatch guard
// is `Any` and can't name the implementor's class args statically. The matched
// runtime instance still carries them, so the dispatched class-owned method
// must resolve its class `T` (here via `reflect.type_of<T>()`).

interface Describable {
  function describe(self) -> string throws never
}

class TypedBox<T> {
    value: T,
    implements Describable {
        function describe(self) -> string {
            return reflect.type_of<T>().to_string();
        }
    }
}

class PlainThing {
    n: int,
    implements Describable {
        function describe(self) -> string {
            return "plain";
        }
    }
}

function generic_behind_nongeneric_iface() -> string {
    let d: Describable = TypedBox<int> { value: 42 };
    d.describe()
}

test "generic_class_behind_nongeneric_interface_reads_type_var" {
    assert.equal(generic_behind_nongeneric_iface(), "int")
}

// ═══════════════════════════════════════════════════════════════════════════
// Inferred call-site type args reach the callee frame at runtime.
//
// The cases above dispatch correctly, but dispatch alone can pass "by luck"
// of arm order even when the instantiation is lost (the unguarded last arm of
// the candidate switch happens to be the right implementor). The cases below
// pin the instantiation itself, so they cannot pass by dispatch luck:
//   • a free generic function must see its inferred `T` via reflect;
//   • an instance built by an inferred static ctor must carry
//     `class_type_args = [int]` — observable through a user-level
//     `match`/IsType test, which consults the args directly.
// Both were broken before TIR began persisting inferred call bindings in
// `CallPlan.type_args` for MIR to seed the callee frame.
// ═══════════════════════════════════════════════════════════════════════════

function free_fn_tname<T>(a: T[]) -> string {
    return reflect.type_of<T>().to_string();
}

function free_fn_inferred_t() -> string {
    return free_fn_tname([1, 2, 3]);
}

test "free_function_sees_inferred_type_arg" {
    assert.equal(free_fn_inferred_t(), "int")
}

// The inferred-ctor instance must pass a runtime `IsType ArrBag<int>` test —
// NOT fall through to the wildcard arm.
function inferred_ctor_instance_is_typed() -> int throws never {
    let s: Bag<int> = ArrBag.make([5, 6, 7]);
    match (s) {
        let a: ArrBag<int> => 1,
        let o: OneBag<int> => 2,
        _ => 9,
    }
}

test "inferred_static_ctor_instance_carries_class_type_args" {
    assert.equal(inferred_ctor_instance_is_typed(), 1)
}

// Lazy-iterator shape end-to-end: inferred static ctor, consumption through
// an inherited default method that drives `self.next()` in a loop, with a
// second implementor present. Returns the COLLECTED COUNT, so a mis-dispatch
// (e.g. `next` resolving to `Lone.next` against an `ArrIter` receiver)
// yields 0 or 1 instead of 3.
interface NextIter<T, E> {
  function next2(self) -> T | IterDone throws E

  function collect2(self) -> T[] throws E {
    let out: T[] = [];
    while (true) {
      match (self.next2()) {
        IterDone => { break; },
        let x: T => { out.push(x); }
      }
    }
    out
  }
}

class IterDone {
}

class ArrIter<T> {
    arr: T[],
    idx: int,

    function of(a: T[]) -> ArrIter<T> throws never {
        return ArrIter<T> { arr: a, idx: 0 };
    }

    implements NextIter<T, never> {
        function next2(self) -> T | IterDone throws never {
            match (self.arr.at(self.idx)) {
                null => IterDone {  },
                let x: T => {
                    self.idx += 1;
                    x
                },
            }
        }
    }
}

class LoneIter<T> {
    v: T,
    used: bool,

    implements NextIter<T, never> {
        function next2(self) -> T | IterDone throws never {
            if (self.used) {
                IterDone {  }
            } else {
                self.used = true;
                self.v
            }
        }
    }
}

function inferred_ctor_collect_count() -> int throws never {
    let it: NextIter<int, never> = ArrIter.of([1, 2, 3]);
    it.collect2().length()
}

test "inferred_ctor_default_method_collects_all" {
    assert.equal(inferred_ctor_collect_count(), 3)
}

// ─── Inferred binding nesting a rigid `Self` must NOT seed `void` ───
//
// Inside an interface default method, `self` has the rigid pinned type `Self`,
// which is never among the caller frame's generic params. An inferred generic
// call whose binding NESTS it (here `T := Self[]` via `echo_type([self])`)
// cannot be resolved to a frame index — the seeding must demote that binding
// to `unknown` (the unseeded status quo), not lower `Self` to `void`.

function echo_type<T>(a: T) -> string {
    return reflect.type_of<T>().to_string();
}

interface SelfEcho {
  function tag(self) -> string throws never
  function nested_self_type(self) -> string throws never {
    return echo_type([self])
  }
}

class EchoA {
    n: int,
    implements SelfEcho {
        function tag(self) -> string {
            return "a";
        }
    }
}

class EchoB {
    s: string,
    implements SelfEcho {
        function tag(self) -> string {
            return "b";
        }
    }
}

function nested_self_inferred_binding() -> string throws never {
    let e: SelfEcho = EchoA { n: 1 };
    e.nested_self_type()
}

// `unknown` (the whole binding demoted) is the pre-existing status quo for an
// unresolvable binding — the regression this guards against is `void[]`, which
// a naive lowering of the nested rigid `Self` produces.
test "nested_self_inferred_binding_not_voided" {
    assert.equal(nested_self_inferred_binding(), "unknown")
}

// Interface dispatch must also carry inferred method-level type args. The
// dispatch arm itself seeds the receiver/interface frame; `CallPlan.type_args`
// supplies the method's inferred `U`.
interface TypeProbe {
  function seen<U>(self, value: U) -> string throws never
}

class ProbeA {
    implements TypeProbe {
        function seen<U>(self, value: U) -> string {
            return reflect.type_of<U>().to_string();
        }
    }
}

class ProbeB {
    implements TypeProbe {
        function seen<U>(self, value: U) -> string {
            return "wrong";
        }
    }
}

function interface_dispatch_inferred_method_type_arg() -> string {
    let p: TypeProbe = ProbeA {  };
    let x: int = 1;
    p.seen(x)
}

test "interface_dispatch_sees_inferred_method_type_arg" {
    assert.equal(interface_dispatch_inferred_method_type_arg(), "int")
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// interfaces_inferred_generic_type_args.baml:1:1 (comment) len=58 "// End-to-end tests for the generic-type-arg dispatch fix."
// interfaces_inferred_generic_type_args.baml:2:1 (comment) len=2 "//"
// interfaces_inferred_generic_type_args.baml:3:1 (comment) len=77 "// A generic class whose `<T>` is *inferred* (rather than written explicitly)"
// interfaces_inferred_generic_type_args.baml:4:1 (comment) len=76 "// must carry the correct runtime `class_type_args`, and a method dispatched"
// interfaces_inferred_generic_type_args.baml:5:1 (comment) len=66 "// through a multi-implementor interface must receive the resolved"
// interfaces_inferred_generic_type_args.baml:6:1 (comment) len=75 "// class/interface type args in its frame. Without this, a method body that"
// interfaces_inferred_generic_type_args.baml:7:1 (comment) len=81 "// reads its enclosing `T` at runtime — `reflect.type_of<T>()`, or constructing"
// interfaces_inferred_generic_type_args.baml:8:1 (comment) len=74 "// `Other<T>{}` — resolves `T` to `unknown`; the new instance gets empty"
// interfaces_inferred_generic_type_args.baml:9:1 (comment) len=78 "// `class_type_args`; and the runtime `IsType` dispatch guard falls through to"
// interfaces_inferred_generic_type_args.baml:10:1 (comment) len=77 "// the WRONG implementor (a silent wrong answer, or a field-access panic when"
// interfaces_inferred_generic_type_args.baml:11:1 (comment) len=45 "// the sibling has a different field layout)."
// interfaces_inferred_generic_type_args.baml:12:1 (comment) len=2 "//"
// interfaces_inferred_generic_type_args.baml:13:1 (comment) len=80 "// Fixed in `baml_compiler2_mir::lower`: the interface-dispatch switch now seeds"
// interfaces_inferred_generic_type_args.baml:14:1 (comment) len=81 "// the callee frame's `type_args` per candidate — a class-owned method gets the"
// interfaces_inferred_generic_type_args.baml:15:1 (comment) len=80 "// implementor's class args (statically when the guard pins them, otherwise from"
// interfaces_inferred_generic_type_args.baml:16:1 (comment) len=79 "// the matched runtime instance via a `BoundMethod`, which covers `Any`/partial"
// interfaces_inferred_generic_type_args.baml:17:1 (comment) len=80 "// guards); an inherited default gets the interface's args. `default.<method>()`"
// interfaces_inferred_generic_type_args.baml:18:1 (comment) len=73 "// forwarding seeds the same way, and `enclosing_generic_params` includes"
// interfaces_inferred_generic_type_args.baml:19:1 (comment) len=40 "// interface params for default methods."
// interfaces_inferred_generic_type_args.baml:20:1 (comment) len=2 "//"
// interfaces_inferred_generic_type_args.baml:21:1 (comment) len=80 "// NOTE: test bodies live in top-level helper functions — locals holding class"
// interfaces_inferred_generic_type_args.baml:22:1 (comment) len=77 "// instances misbehave when compared/passed inside `test { ... }` blocks (the"
// interfaces_inferred_generic_type_args.baml:23:1 (comment) len=78 "// same VM local-boxing caveat documented in `ns_class_type_args_at_runtime`)."
// interfaces_inferred_generic_type_args.baml:25:1 (comment) len=228 "// ═══════════════════════════════════════════════════════════════════════════"
// interfaces_inferred_generic_type_args.baml:26:1 (comment) len=74 "// Showcase: a lazy cursor protocol — the exact shape `baml.iter` needs."
// interfaces_inferred_generic_type_args.baml:27:1 (comment) len=2 "//"
// interfaces_inferred_generic_type_args.baml:28:1 (comment) len=81 "// `Cursor<T>` is a generic interface with a default `take_one(self)` that builds"
// interfaces_inferred_generic_type_args.baml:29:1 (comment) len=80 "// a *new* generic adapter (`SingleCursor<T>`) from the interface's element type"
// interfaces_inferred_generic_type_args.baml:30:1 (comment) len=81 "// `T`. The two leaf implementors have different field layouts, so a mis-dispatch"
// interfaces_inferred_generic_type_args.baml:31:1 (comment) len=79 "// doesn't merely return a wrong value — it reads the wrong field and panics."
// interfaces_inferred_generic_type_args.baml:32:1 (comment) len=2 "//"
// interfaces_inferred_generic_type_args.baml:33:1 (comment) len=79 "// The whole pipeline runs through the `Cursor<int>` interface, and the `<int>`"
// interfaces_inferred_generic_type_args.baml:34:1 (comment) len=78 "// is never written after the first annotation: `ArrayCursor.of([...])` infers"
// interfaces_inferred_generic_type_args.baml:35:1 (comment) len=79 "// it, and the default method must propagate it into the `SingleCursor<int>` it"
// interfaces_inferred_generic_type_args.baml:36:1 (comment) len=76 "// constructs. Correctness depends entirely on the inferred `<int>` reaching"
// interfaces_inferred_generic_type_args.baml:37:1 (comment) len=25 "// each dispatched frame."
// interfaces_inferred_generic_type_args.baml:38:1 (comment) len=228 "// ═══════════════════════════════════════════════════════════════════════════"
// interfaces_inferred_generic_type_args.baml:40:1 (keyword) len=9 "interface"
// interfaces_inferred_generic_type_args.baml:40:11 (interface) [declaration] len=6 "Cursor"
// interfaces_inferred_generic_type_args.baml:40:17 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:40:18 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:40:19 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:41:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:41:12 (method) [declaration] len=4 "head"
// interfaces_inferred_generic_type_args.baml:41:17 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:41:26 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:41:28 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:41:35 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:43:3 (comment) len=74 "// Default method, inherited by every implementor. It constructs a generic"
// interfaces_inferred_generic_type_args.baml:44:3 (comment) len=78 "// `SingleCursor<T>` from the interface's own `T`. The resulting instance MUST"
// interfaces_inferred_generic_type_args.baml:45:3 (comment) len=75 "// carry `class_type_args = [T]`; otherwise the `single.head()` dispatch in"
// interfaces_inferred_generic_type_args.baml:46:3 (comment) len=75 "// `take_one_then_head` resolves to `ArrayCursor.head` — which indexes an"
// interfaces_inferred_generic_type_args.baml:47:3 (comment) len=63 "// `arr` field that `SingleCursor` does not have — instead of"
// interfaces_inferred_generic_type_args.baml:48:3 (comment) len=23 "// `SingleCursor.head`."
// interfaces_inferred_generic_type_args.baml:49:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:49:12 (method) [declaration] len=8 "take_one"
// interfaces_inferred_generic_type_args.baml:49:21 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:49:30 (interface) len=6 "Cursor"
// interfaces_inferred_generic_type_args.baml:49:36 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:49:37 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:49:38 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:49:40 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:49:47 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:50:5 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:50:12 (class) len=12 "SingleCursor"
// interfaces_inferred_generic_type_args.baml:50:24 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:50:25 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:50:26 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:50:30 (property) len=5 "value"
// interfaces_inferred_generic_type_args.baml:50:37 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:50:42 (method) len=4 "head"
// interfaces_inferred_generic_type_args.baml:54:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:54:7 (class) [declaration] len=11 "ArrayCursor"
// interfaces_inferred_generic_type_args.baml:54:18 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:54:19 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:54:20 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:55:5 (property) [declaration] len=3 "arr"
// interfaces_inferred_generic_type_args.baml:55:10 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:56:5 (property) [declaration] len=3 "idx"
// interfaces_inferred_generic_type_args.baml:56:10 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:58:5 (comment) len=72 "// Static constructor: `T` is INFERRED from the argument, never written."
// interfaces_inferred_generic_type_args.baml:59:5 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:59:14 (method) [declaration] len=2 "of"
// interfaces_inferred_generic_type_args.baml:59:17 (parameter) [declaration] len=5 "items"
// interfaces_inferred_generic_type_args.baml:59:24 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:59:32 (class) len=11 "ArrayCursor"
// interfaces_inferred_generic_type_args.baml:59:43 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:59:44 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:59:45 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:60:9 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:60:16 (class) len=11 "ArrayCursor"
// interfaces_inferred_generic_type_args.baml:60:27 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:60:28 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:60:29 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:60:33 (property) len=3 "arr"
// interfaces_inferred_generic_type_args.baml:60:38 (parameter) len=5 "items"
// interfaces_inferred_generic_type_args.baml:60:45 (property) len=3 "idx"
// interfaces_inferred_generic_type_args.baml:60:50 (number) len=1 "0"
// interfaces_inferred_generic_type_args.baml:63:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:63:16 (interface) len=6 "Cursor"
// interfaces_inferred_generic_type_args.baml:63:22 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:63:23 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:63:24 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:64:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:64:18 (method) [declaration] len=4 "head"
// interfaces_inferred_generic_type_args.baml:64:23 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:64:32 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:64:34 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:64:41 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:65:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:65:20 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:65:25 (property) len=3 "arr"
// interfaces_inferred_generic_type_args.baml:65:29 (number) len=1 "0"
// interfaces_inferred_generic_type_args.baml:70:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:70:7 (class) [declaration] len=12 "SingleCursor"
// interfaces_inferred_generic_type_args.baml:70:19 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:70:20 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:70:21 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:71:5 (property) [declaration] len=5 "value"
// interfaces_inferred_generic_type_args.baml:71:12 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:73:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:73:16 (interface) len=6 "Cursor"
// interfaces_inferred_generic_type_args.baml:73:22 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:73:23 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:73:24 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:74:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:74:18 (method) [declaration] len=4 "head"
// interfaces_inferred_generic_type_args.baml:74:23 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:74:32 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:74:34 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:74:41 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:75:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:75:20 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:75:25 (property) len=5 "value"
// interfaces_inferred_generic_type_args.baml:80:1 (comment) len=78 "// Build an `ArrayCursor<int>` with an inferred `<int>`, narrow it to a single"
// interfaces_inferred_generic_type_args.baml:81:1 (comment) len=81 "// element via the inherited default method, then read the head back — entirely"
// interfaces_inferred_generic_type_args.baml:82:1 (comment) len=80 "// through the `Cursor<int>` interface. Exercises inferred static-ctor type args"
// interfaces_inferred_generic_type_args.baml:83:1 (comment) len=71 "// AND a default method that builds a generic from the interface's `T`."
// interfaces_inferred_generic_type_args.baml:84:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:84:10 (function) [declaration] len=18 "take_one_then_head"
// interfaces_inferred_generic_type_args.baml:84:34 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:84:38 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:84:45 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:85:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:85:9 (variable) [declaration] len=6 "cursor"
// interfaces_inferred_generic_type_args.baml:85:17 (interface) len=6 "Cursor"
// interfaces_inferred_generic_type_args.baml:85:23 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:85:24 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:85:27 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:85:29 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:85:31 (class) len=11 "ArrayCursor"
// interfaces_inferred_generic_type_args.baml:85:43 (method) len=2 "of"
// interfaces_inferred_generic_type_args.baml:85:47 (number) len=2 "10"
// interfaces_inferred_generic_type_args.baml:85:51 (number) len=2 "20"
// interfaces_inferred_generic_type_args.baml:85:55 (number) len=2 "30"
// interfaces_inferred_generic_type_args.baml:86:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:86:9 (variable) [declaration] len=6 "single"
// interfaces_inferred_generic_type_args.baml:86:17 (interface) len=6 "Cursor"
// interfaces_inferred_generic_type_args.baml:86:23 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:86:24 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:86:27 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:86:29 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:86:31 (variable) len=6 "cursor"
// interfaces_inferred_generic_type_args.baml:86:38 (method) len=8 "take_one"
// interfaces_inferred_generic_type_args.baml:87:5 (variable) len=6 "single"
// interfaces_inferred_generic_type_args.baml:87:12 (method) len=4 "head"
// interfaces_inferred_generic_type_args.baml:90:1 (keyword) len=4 "test"
// interfaces_inferred_generic_type_args.baml:90:6 (string) len=31 "\"showcase_lazy_cursor_pipeline\""
// interfaces_inferred_generic_type_args.baml:91:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_inferred_generic_type_args.baml:91:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:91:12 (function) len=5 "equal"
// interfaces_inferred_generic_type_args.baml:91:18 (function) len=18 "take_one_then_head"
// interfaces_inferred_generic_type_args.baml:91:40 (number) len=2 "10"
// interfaces_inferred_generic_type_args.baml:94:1 (comment) len=228 "// ═══════════════════════════════════════════════════════════════════════════"
// interfaces_inferred_generic_type_args.baml:95:1 (comment) len=27 "// Focused regression cases"
// interfaces_inferred_generic_type_args.baml:96:1 (comment) len=228 "// ═══════════════════════════════════════════════════════════════════════════"
// interfaces_inferred_generic_type_args.baml:98:1 (comment) len=92 "// ─── Inferred static constructor + non-generic interface, two implementors ───"
// interfaces_inferred_generic_type_args.baml:100:1 (keyword) len=9 "interface"
// interfaces_inferred_generic_type_args.baml:100:11 (interface) [declaration] len=6 "Animal"
// interfaces_inferred_generic_type_args.baml:101:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:101:12 (method) [declaration] len=5 "speak"
// interfaces_inferred_generic_type_args.baml:101:18 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:101:27 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:101:34 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:101:41 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:104:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:104:7 (class) [declaration] len=3 "Dog"
// interfaces_inferred_generic_type_args.baml:104:10 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:104:11 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:104:12 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:105:5 (property) [declaration] len=5 "value"
// interfaces_inferred_generic_type_args.baml:105:12 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:107:5 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:107:14 (method) [declaration] len=4 "make"
// interfaces_inferred_generic_type_args.baml:107:19 (parameter) [declaration] len=1 "v"
// interfaces_inferred_generic_type_args.baml:107:22 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:107:28 (class) len=3 "Dog"
// interfaces_inferred_generic_type_args.baml:107:31 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:107:32 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:107:33 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:108:9 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:108:16 (class) len=3 "Dog"
// interfaces_inferred_generic_type_args.baml:108:19 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:108:20 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:108:21 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:108:25 (property) len=5 "value"
// interfaces_inferred_generic_type_args.baml:108:32 (parameter) len=1 "v"
// interfaces_inferred_generic_type_args.baml:111:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:111:16 (interface) len=6 "Animal"
// interfaces_inferred_generic_type_args.baml:112:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:112:18 (method) [declaration] len=5 "speak"
// interfaces_inferred_generic_type_args.baml:112:24 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:112:33 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:113:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:113:20 (string) len=6 "\"woof\""
// interfaces_inferred_generic_type_args.baml:118:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:118:7 (class) [declaration] len=3 "Cat"
// interfaces_inferred_generic_type_args.baml:119:5 (property) [declaration] len=10 "meow_count"
// interfaces_inferred_generic_type_args.baml:119:17 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:121:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:121:16 (interface) len=6 "Animal"
// interfaces_inferred_generic_type_args.baml:122:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:122:18 (method) [declaration] len=5 "speak"
// interfaces_inferred_generic_type_args.baml:122:24 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:122:33 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:123:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:123:20 (string) len=6 "\"meow\""
// interfaces_inferred_generic_type_args.baml:128:1 (comment) len=77 "// `Dog.make(42)` infers `Dog<int>`; dispatched through `Animal` (which `Cat`"
// interfaces_inferred_generic_type_args.baml:129:1 (comment) len=54 "// also implements) it must still pick `Dog`'s method."
// interfaces_inferred_generic_type_args.baml:130:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:130:10 (function) [declaration] len=26 "inferred_static_ctor_speak"
// interfaces_inferred_generic_type_args.baml:130:42 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:131:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:131:9 (variable) [declaration] len=1 "d"
// interfaces_inferred_generic_type_args.baml:131:12 (interface) len=6 "Animal"
// interfaces_inferred_generic_type_args.baml:131:19 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:131:21 (class) len=3 "Dog"
// interfaces_inferred_generic_type_args.baml:131:25 (method) len=4 "make"
// interfaces_inferred_generic_type_args.baml:131:30 (number) len=2 "42"
// interfaces_inferred_generic_type_args.baml:132:5 (variable) len=1 "d"
// interfaces_inferred_generic_type_args.baml:132:7 (method) len=5 "speak"
// interfaces_inferred_generic_type_args.baml:135:1 (keyword) len=4 "test"
// interfaces_inferred_generic_type_args.baml:135:6 (string) len=56 "\"inferred_static_ctor_dispatches_to_correct_implementor\""
// interfaces_inferred_generic_type_args.baml:136:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_inferred_generic_type_args.baml:136:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:136:12 (function) len=5 "equal"
// interfaces_inferred_generic_type_args.baml:136:18 (function) len=26 "inferred_static_ctor_speak"
// interfaces_inferred_generic_type_args.baml:136:48 (string) len=6 "\"woof\""
// interfaces_inferred_generic_type_args.baml:139:1 (comment) len=89 "// ─── Inferred static constructor + generic interface, mismatched shapes ───"
// interfaces_inferred_generic_type_args.baml:141:1 (keyword) len=9 "interface"
// interfaces_inferred_generic_type_args.baml:141:11 (interface) [declaration] len=3 "Bag"
// interfaces_inferred_generic_type_args.baml:141:14 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:141:15 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:141:16 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:142:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:142:12 (method) [declaration] len=5 "first"
// interfaces_inferred_generic_type_args.baml:142:18 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:142:27 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:142:29 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:142:36 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:145:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:145:7 (class) [declaration] len=6 "ArrBag"
// interfaces_inferred_generic_type_args.baml:145:13 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:145:14 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:145:15 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:146:5 (property) [declaration] len=3 "arr"
// interfaces_inferred_generic_type_args.baml:146:10 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:147:5 (property) [declaration] len=3 "idx"
// interfaces_inferred_generic_type_args.baml:147:10 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:149:5 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:149:14 (method) [declaration] len=4 "make"
// interfaces_inferred_generic_type_args.baml:149:19 (parameter) [declaration] len=1 "a"
// interfaces_inferred_generic_type_args.baml:149:22 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:149:30 (class) len=6 "ArrBag"
// interfaces_inferred_generic_type_args.baml:149:36 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:149:37 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:149:38 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:150:9 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:150:16 (class) len=6 "ArrBag"
// interfaces_inferred_generic_type_args.baml:150:22 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:150:23 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:150:24 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:150:28 (property) len=3 "arr"
// interfaces_inferred_generic_type_args.baml:150:33 (parameter) len=1 "a"
// interfaces_inferred_generic_type_args.baml:150:36 (property) len=3 "idx"
// interfaces_inferred_generic_type_args.baml:150:41 (number) len=1 "0"
// interfaces_inferred_generic_type_args.baml:153:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:153:16 (interface) len=3 "Bag"
// interfaces_inferred_generic_type_args.baml:153:19 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:153:20 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:153:21 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:154:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:154:18 (method) [declaration] len=5 "first"
// interfaces_inferred_generic_type_args.baml:154:24 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:154:33 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:154:35 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:154:42 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:155:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:155:20 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:155:25 (property) len=3 "arr"
// interfaces_inferred_generic_type_args.baml:155:29 (number) len=1 "0"
// interfaces_inferred_generic_type_args.baml:160:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:160:7 (class) [declaration] len=6 "OneBag"
// interfaces_inferred_generic_type_args.baml:160:13 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:160:14 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:160:15 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:161:5 (property) [declaration] len=4 "only"
// interfaces_inferred_generic_type_args.baml:161:11 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:163:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:163:16 (interface) len=3 "Bag"
// interfaces_inferred_generic_type_args.baml:163:19 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:163:20 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:163:21 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:164:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:164:18 (method) [declaration] len=5 "first"
// interfaces_inferred_generic_type_args.baml:164:24 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:164:33 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:164:35 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:164:42 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:165:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:165:20 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:165:25 (property) len=4 "only"
// interfaces_inferred_generic_type_args.baml:170:1 (comment) len=72 "// A mis-dispatch would read `only` off an `ArrBag` (or index `arr` on a"
// interfaces_inferred_generic_type_args.baml:171:1 (comment) len=80 "// `OneBag`) — different layouts, so the wrong arm panics rather than silently"
// interfaces_inferred_generic_type_args.baml:172:1 (comment) len=25 "// returning a bad value."
// interfaces_inferred_generic_type_args.baml:173:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:173:10 (function) [declaration] len=26 "inferred_static_ctor_first"
// interfaces_inferred_generic_type_args.baml:173:42 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:173:46 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:173:53 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:174:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:174:9 (variable) [declaration] len=1 "s"
// interfaces_inferred_generic_type_args.baml:174:12 (interface) len=3 "Bag"
// interfaces_inferred_generic_type_args.baml:174:15 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:174:16 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:174:19 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:174:21 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:174:23 (class) len=6 "ArrBag"
// interfaces_inferred_generic_type_args.baml:174:30 (method) len=4 "make"
// interfaces_inferred_generic_type_args.baml:174:36 (number) len=1 "5"
// interfaces_inferred_generic_type_args.baml:174:39 (number) len=1 "6"
// interfaces_inferred_generic_type_args.baml:174:42 (number) len=1 "7"
// interfaces_inferred_generic_type_args.baml:175:5 (variable) len=1 "s"
// interfaces_inferred_generic_type_args.baml:175:7 (method) len=5 "first"
// interfaces_inferred_generic_type_args.baml:178:1 (keyword) len=4 "test"
// interfaces_inferred_generic_type_args.baml:178:6 (string) len=59 "\"inferred_static_ctor_dispatches_through_generic_interface\""
// interfaces_inferred_generic_type_args.baml:179:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_inferred_generic_type_args.baml:179:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:179:12 (function) len=5 "equal"
// interfaces_inferred_generic_type_args.baml:179:18 (function) len=26 "inferred_static_ctor_first"
// interfaces_inferred_generic_type_args.baml:179:48 (number) len=1 "5"
// interfaces_inferred_generic_type_args.baml:182:1 (comment) len=92 "// ─── Interface default method can read the interface's type var at runtime ───"
// interfaces_inferred_generic_type_args.baml:184:1 (keyword) len=9 "interface"
// interfaces_inferred_generic_type_args.baml:184:11 (interface) [declaration] len=5 "Named"
// interfaces_inferred_generic_type_args.baml:184:16 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:184:17 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:184:18 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:185:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:185:12 (method) [declaration] len=5 "first"
// interfaces_inferred_generic_type_args.baml:185:18 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:185:27 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:185:29 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:185:36 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:186:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:186:12 (method) [declaration] len=5 "tname"
// interfaces_inferred_generic_type_args.baml:186:18 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:186:27 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:186:34 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:186:41 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:187:5 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:187:12 (namespace) [defaultLibrary] len=7 "reflect"
// interfaces_inferred_generic_type_args.baml:187:19 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:187:20 (function) len=7 "type_of"
// interfaces_inferred_generic_type_args.baml:187:27 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:187:28 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:187:29 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:187:33 (method) len=9 "to_string"
// interfaces_inferred_generic_type_args.baml:191:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:191:7 (class) [declaration] len=8 "ArrNamed"
// interfaces_inferred_generic_type_args.baml:191:15 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:191:16 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:191:17 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:192:5 (property) [declaration] len=3 "arr"
// interfaces_inferred_generic_type_args.baml:192:10 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:193:5 (property) [declaration] len=3 "idx"
// interfaces_inferred_generic_type_args.baml:193:10 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:194:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:194:16 (interface) len=5 "Named"
// interfaces_inferred_generic_type_args.baml:194:21 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:194:22 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:194:23 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:195:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:195:18 (method) [declaration] len=5 "first"
// interfaces_inferred_generic_type_args.baml:195:24 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:195:33 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:195:35 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:195:42 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:196:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:196:20 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:196:25 (property) len=3 "arr"
// interfaces_inferred_generic_type_args.baml:196:29 (number) len=1 "0"
// interfaces_inferred_generic_type_args.baml:201:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:201:7 (class) [declaration] len=8 "OneNamed"
// interfaces_inferred_generic_type_args.baml:201:15 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:201:16 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:201:17 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:202:5 (property) [declaration] len=4 "only"
// interfaces_inferred_generic_type_args.baml:202:11 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:203:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:203:16 (interface) len=5 "Named"
// interfaces_inferred_generic_type_args.baml:203:21 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:203:22 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:203:23 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:204:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:204:18 (method) [declaration] len=5 "first"
// interfaces_inferred_generic_type_args.baml:204:24 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:204:33 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:204:35 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:204:42 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:205:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:205:20 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:205:25 (property) len=4 "only"
// interfaces_inferred_generic_type_args.baml:210:1 (comment) len=75 "// `tname` is a default method dispatched through `Named<int>`; it must see"
// interfaces_inferred_generic_type_args.baml:211:1 (comment) len=36 "// `T == int`, not `unknown`/`void`."
// interfaces_inferred_generic_type_args.baml:212:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:212:10 (function) [declaration] len=27 "default_method_typevar_name"
// interfaces_inferred_generic_type_args.baml:212:43 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:212:50 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:212:57 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:213:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:213:9 (variable) [declaration] len=1 "s"
// interfaces_inferred_generic_type_args.baml:213:12 (interface) len=5 "Named"
// interfaces_inferred_generic_type_args.baml:213:17 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:213:18 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:213:21 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:213:23 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:213:25 (class) len=8 "ArrNamed"
// interfaces_inferred_generic_type_args.baml:213:33 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:213:34 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:213:37 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:213:41 (property) len=3 "arr"
// interfaces_inferred_generic_type_args.baml:213:47 (number) len=1 "5"
// interfaces_inferred_generic_type_args.baml:213:50 (number) len=1 "6"
// interfaces_inferred_generic_type_args.baml:213:53 (number) len=1 "7"
// interfaces_inferred_generic_type_args.baml:213:57 (property) len=3 "idx"
// interfaces_inferred_generic_type_args.baml:213:62 (number) len=1 "0"
// interfaces_inferred_generic_type_args.baml:214:5 (variable) len=1 "s"
// interfaces_inferred_generic_type_args.baml:214:7 (method) len=5 "tname"
// interfaces_inferred_generic_type_args.baml:217:1 (keyword) len=4 "test"
// interfaces_inferred_generic_type_args.baml:217:6 (string) len=50 "\"interface_default_method_sees_interface_type_var\""
// interfaces_inferred_generic_type_args.baml:218:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_inferred_generic_type_args.baml:218:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:218:12 (function) len=5 "equal"
// interfaces_inferred_generic_type_args.baml:218:18 (function) len=27 "default_method_typevar_name"
// interfaces_inferred_generic_type_args.baml:218:49 (string) len=5 "\"int\""
// interfaces_inferred_generic_type_args.baml:221:1 (comment) len=92 "// ─── Class-owned (non-default) method that builds a generic from class `T` ───"
// interfaces_inferred_generic_type_args.baml:223:1 (keyword) len=9 "interface"
// interfaces_inferred_generic_type_args.baml:223:11 (interface) [declaration] len=4 "Wrap"
// interfaces_inferred_generic_type_args.baml:223:15 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:223:16 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:223:17 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:224:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:224:12 (method) [declaration] len=5 "first"
// interfaces_inferred_generic_type_args.baml:224:18 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:224:27 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:224:29 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:224:36 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:227:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:227:7 (class) [declaration] len=7 "SrcWrap"
// interfaces_inferred_generic_type_args.baml:227:14 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:227:15 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:227:16 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:228:5 (property) [declaration] len=3 "arr"
// interfaces_inferred_generic_type_args.baml:228:10 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:229:5 (property) [declaration] len=3 "idx"
// interfaces_inferred_generic_type_args.baml:229:10 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:231:5 (comment) len=70 "// A regular class method (not an interface default) that constructs a"
// interfaces_inferred_generic_type_args.baml:232:5 (comment) len=47 "// generic `Boxed<T>` from the class-level `T`."
// interfaces_inferred_generic_type_args.baml:233:5 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:233:14 (method) [declaration] len=5 "boxed"
// interfaces_inferred_generic_type_args.baml:233:20 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:233:29 (interface) len=4 "Wrap"
// interfaces_inferred_generic_type_args.baml:233:33 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:233:34 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:233:35 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:233:37 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:233:44 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:234:9 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:234:16 (class) len=5 "Boxed"
// interfaces_inferred_generic_type_args.baml:234:21 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:234:22 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:234:23 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:234:27 (property) len=3 "val"
// interfaces_inferred_generic_type_args.baml:234:32 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:234:37 (property) len=3 "arr"
// interfaces_inferred_generic_type_args.baml:234:41 (number) len=1 "0"
// interfaces_inferred_generic_type_args.baml:237:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:237:16 (interface) len=4 "Wrap"
// interfaces_inferred_generic_type_args.baml:237:20 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:237:21 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:237:22 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:238:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:238:18 (method) [declaration] len=5 "first"
// interfaces_inferred_generic_type_args.baml:238:24 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:238:33 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:238:35 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:238:42 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:239:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:239:20 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:239:25 (property) len=3 "arr"
// interfaces_inferred_generic_type_args.baml:239:29 (number) len=1 "0"
// interfaces_inferred_generic_type_args.baml:244:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:244:7 (class) [declaration] len=5 "Boxed"
// interfaces_inferred_generic_type_args.baml:244:12 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:244:13 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:244:14 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:245:5 (property) [declaration] len=3 "val"
// interfaces_inferred_generic_type_args.baml:245:10 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:246:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:246:16 (interface) len=4 "Wrap"
// interfaces_inferred_generic_type_args.baml:246:20 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:246:21 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:246:22 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:247:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:247:18 (method) [declaration] len=5 "first"
// interfaces_inferred_generic_type_args.baml:247:24 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:247:33 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:247:35 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:247:42 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:248:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:248:20 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:248:25 (property) len=3 "val"
// interfaces_inferred_generic_type_args.baml:253:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:253:10 (function) [declaration] len=27 "class_method_builds_generic"
// interfaces_inferred_generic_type_args.baml:253:43 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:253:47 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:253:54 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:254:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:254:9 (variable) [declaration] len=1 "s"
// interfaces_inferred_generic_type_args.baml:254:12 (class) len=7 "SrcWrap"
// interfaces_inferred_generic_type_args.baml:254:19 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:254:20 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:254:23 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:254:25 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:254:27 (class) len=7 "SrcWrap"
// interfaces_inferred_generic_type_args.baml:254:34 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:254:35 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:254:38 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:254:42 (property) len=3 "arr"
// interfaces_inferred_generic_type_args.baml:254:48 (number) len=1 "5"
// interfaces_inferred_generic_type_args.baml:254:51 (number) len=1 "6"
// interfaces_inferred_generic_type_args.baml:254:54 (number) len=1 "7"
// interfaces_inferred_generic_type_args.baml:254:58 (property) len=3 "idx"
// interfaces_inferred_generic_type_args.baml:254:63 (number) len=1 "0"
// interfaces_inferred_generic_type_args.baml:255:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:255:9 (variable) [declaration] len=1 "w"
// interfaces_inferred_generic_type_args.baml:255:12 (interface) len=4 "Wrap"
// interfaces_inferred_generic_type_args.baml:255:16 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:255:17 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:255:20 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:255:22 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:255:24 (variable) len=1 "s"
// interfaces_inferred_generic_type_args.baml:255:26 (method) len=5 "boxed"
// interfaces_inferred_generic_type_args.baml:256:5 (variable) len=1 "w"
// interfaces_inferred_generic_type_args.baml:256:7 (method) len=5 "first"
// interfaces_inferred_generic_type_args.baml:259:1 (keyword) len=4 "test"
// interfaces_inferred_generic_type_args.baml:259:6 (string) len=48 "\"class_method_builds_generic_via_class_type_var\""
// interfaces_inferred_generic_type_args.baml:260:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_inferred_generic_type_args.baml:260:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:260:12 (function) len=5 "equal"
// interfaces_inferred_generic_type_args.baml:260:18 (function) len=27 "class_method_builds_generic"
// interfaces_inferred_generic_type_args.baml:260:49 (number) len=1 "5"
// interfaces_inferred_generic_type_args.baml:263:1 (comment) len=85 "// ─── `default.<method>()` forwards into a generic interface default ───"
// interfaces_inferred_generic_type_args.baml:264:1 (comment) len=2 "//"
// interfaces_inferred_generic_type_args.baml:265:1 (comment) len=78 "// Calling the interface's default implementation explicitly (the `super`-like"
// interfaces_inferred_generic_type_args.baml:266:1 (comment) len=75 "// `default.<method>()` form) from inside an override must seed the default"
// interfaces_inferred_generic_type_args.baml:267:1 (comment) len=79 "// body's frame with the interface's type args, so a generic default that reads"
// interfaces_inferred_generic_type_args.baml:268:1 (comment) len=58 "// `T` at runtime resolves it instead of seeing `unknown`."
// interfaces_inferred_generic_type_args.baml:270:1 (keyword) len=9 "interface"
// interfaces_inferred_generic_type_args.baml:270:11 (interface) [declaration] len=7 "Counter"
// interfaces_inferred_generic_type_args.baml:270:18 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:270:19 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:270:20 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:271:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:271:12 (method) [declaration] len=12 "element_type"
// interfaces_inferred_generic_type_args.baml:271:25 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:271:34 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:271:41 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:271:48 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:272:5 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:272:12 (namespace) [defaultLibrary] len=7 "reflect"
// interfaces_inferred_generic_type_args.baml:272:19 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:272:20 (function) len=7 "type_of"
// interfaces_inferred_generic_type_args.baml:272:27 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:272:28 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:272:29 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:272:33 (method) len=9 "to_string"
// interfaces_inferred_generic_type_args.baml:274:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:274:12 (method) [declaration] len=8 "describe"
// interfaces_inferred_generic_type_args.baml:274:21 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:274:30 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:274:37 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:274:44 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:277:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:277:7 (class) [declaration] len=10 "IntCounter"
// interfaces_inferred_generic_type_args.baml:277:17 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:277:18 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:277:19 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:278:5 (property) [declaration] len=5 "value"
// interfaces_inferred_generic_type_args.baml:278:12 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:279:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:279:16 (interface) len=7 "Counter"
// interfaces_inferred_generic_type_args.baml:279:23 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:279:24 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:279:25 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:280:9 (comment) len=74 "// Override `describe` but delegate to the generic default `element_type`."
// interfaces_inferred_generic_type_args.baml:281:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:281:18 (method) [declaration] len=8 "describe"
// interfaces_inferred_generic_type_args.baml:281:27 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:281:36 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:282:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:287:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:287:7 (class) [declaration] len=10 "StrCounter"
// interfaces_inferred_generic_type_args.baml:287:17 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:287:18 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:287:19 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:288:5 (property) [declaration] len=5 "label"
// interfaces_inferred_generic_type_args.baml:288:12 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:289:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:289:16 (interface) len=7 "Counter"
// interfaces_inferred_generic_type_args.baml:289:23 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:289:24 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:289:25 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:290:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:290:18 (method) [declaration] len=8 "describe"
// interfaces_inferred_generic_type_args.baml:290:27 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:290:36 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:291:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:291:20 (string) len=5 "\"str\""
// interfaces_inferred_generic_type_args.baml:296:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:296:10 (function) [declaration] len=30 "default_forward_reads_type_var"
// interfaces_inferred_generic_type_args.baml:296:46 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:297:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:297:9 (variable) [declaration] len=1 "c"
// interfaces_inferred_generic_type_args.baml:297:12 (interface) len=7 "Counter"
// interfaces_inferred_generic_type_args.baml:297:19 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:297:20 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:297:23 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:297:25 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:297:27 (class) len=10 "IntCounter"
// interfaces_inferred_generic_type_args.baml:297:37 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:297:38 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:297:41 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:297:45 (property) len=5 "value"
// interfaces_inferred_generic_type_args.baml:297:52 (number) len=1 "7"
// interfaces_inferred_generic_type_args.baml:298:5 (variable) len=1 "c"
// interfaces_inferred_generic_type_args.baml:298:7 (method) len=8 "describe"
// interfaces_inferred_generic_type_args.baml:301:1 (keyword) len=4 "test"
// interfaces_inferred_generic_type_args.baml:301:6 (string) len=49 "\"default_method_forward_reads_interface_type_var\""
// interfaces_inferred_generic_type_args.baml:302:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_inferred_generic_type_args.baml:302:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:302:12 (function) len=5 "equal"
// interfaces_inferred_generic_type_args.baml:302:18 (function) len=30 "default_forward_reads_type_var"
// interfaces_inferred_generic_type_args.baml:302:52 (string) len=5 "\"int\""
// interfaces_inferred_generic_type_args.baml:305:1 (comment) len=87 "// ─── Generic class behind a NON-generic interface reads its class `T` ───"
// interfaces_inferred_generic_type_args.baml:306:1 (comment) len=2 "//"
// interfaces_inferred_generic_type_args.baml:307:1 (comment) len=79 "// The interface request pins no type args (it has none), so the dispatch guard"
// interfaces_inferred_generic_type_args.baml:308:1 (comment) len=79 "// is `Any` and can't name the implementor's class args statically. The matched"
// interfaces_inferred_generic_type_args.baml:309:1 (comment) len=76 "// runtime instance still carries them, so the dispatched class-owned method"
// interfaces_inferred_generic_type_args.baml:310:1 (comment) len=64 "// must resolve its class `T` (here via `reflect.type_of<T>()`)."
// interfaces_inferred_generic_type_args.baml:312:1 (keyword) len=9 "interface"
// interfaces_inferred_generic_type_args.baml:312:11 (interface) [declaration] len=11 "Describable"
// interfaces_inferred_generic_type_args.baml:313:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:313:12 (method) [declaration] len=8 "describe"
// interfaces_inferred_generic_type_args.baml:313:21 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:313:30 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:313:37 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:313:44 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:316:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:316:7 (class) [declaration] len=8 "TypedBox"
// interfaces_inferred_generic_type_args.baml:316:15 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:316:16 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:316:17 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:317:5 (property) [declaration] len=5 "value"
// interfaces_inferred_generic_type_args.baml:317:12 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:318:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:318:16 (interface) len=11 "Describable"
// interfaces_inferred_generic_type_args.baml:319:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:319:18 (method) [declaration] len=8 "describe"
// interfaces_inferred_generic_type_args.baml:319:27 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:319:36 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:320:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:320:20 (namespace) [defaultLibrary] len=7 "reflect"
// interfaces_inferred_generic_type_args.baml:320:27 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:320:28 (function) len=7 "type_of"
// interfaces_inferred_generic_type_args.baml:320:35 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:320:36 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:320:37 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:320:41 (method) len=9 "to_string"
// interfaces_inferred_generic_type_args.baml:325:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:325:7 (class) [declaration] len=10 "PlainThing"
// interfaces_inferred_generic_type_args.baml:326:5 (property) [declaration] len=1 "n"
// interfaces_inferred_generic_type_args.baml:326:8 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:327:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:327:16 (interface) len=11 "Describable"
// interfaces_inferred_generic_type_args.baml:328:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:328:18 (method) [declaration] len=8 "describe"
// interfaces_inferred_generic_type_args.baml:328:27 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:328:36 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:329:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:329:20 (string) len=7 "\"plain\""
// interfaces_inferred_generic_type_args.baml:334:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:334:10 (function) [declaration] len=31 "generic_behind_nongeneric_iface"
// interfaces_inferred_generic_type_args.baml:334:47 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:335:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:335:9 (variable) [declaration] len=1 "d"
// interfaces_inferred_generic_type_args.baml:335:12 (interface) len=11 "Describable"
// interfaces_inferred_generic_type_args.baml:335:24 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:335:26 (class) len=8 "TypedBox"
// interfaces_inferred_generic_type_args.baml:335:34 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:335:35 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:335:38 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:335:42 (property) len=5 "value"
// interfaces_inferred_generic_type_args.baml:335:49 (number) len=2 "42"
// interfaces_inferred_generic_type_args.baml:336:5 (variable) len=1 "d"
// interfaces_inferred_generic_type_args.baml:336:7 (method) len=8 "describe"
// interfaces_inferred_generic_type_args.baml:339:1 (keyword) len=4 "test"
// interfaces_inferred_generic_type_args.baml:339:6 (string) len=58 "\"generic_class_behind_nongeneric_interface_reads_type_var\""
// interfaces_inferred_generic_type_args.baml:340:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_inferred_generic_type_args.baml:340:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:340:12 (function) len=5 "equal"
// interfaces_inferred_generic_type_args.baml:340:18 (function) len=31 "generic_behind_nongeneric_iface"
// interfaces_inferred_generic_type_args.baml:340:53 (string) len=5 "\"int\""
// interfaces_inferred_generic_type_args.baml:343:1 (comment) len=228 "// ═══════════════════════════════════════════════════════════════════════════"
// interfaces_inferred_generic_type_args.baml:344:1 (comment) len=66 "// Inferred call-site type args reach the callee frame at runtime."
// interfaces_inferred_generic_type_args.baml:345:1 (comment) len=2 "//"
// interfaces_inferred_generic_type_args.baml:346:1 (comment) len=76 "// The cases above dispatch correctly, but dispatch alone can pass \"by luck\""
// interfaces_inferred_generic_type_args.baml:347:1 (comment) len=78 "// of arm order even when the instantiation is lost (the unguarded last arm of"
// interfaces_inferred_generic_type_args.baml:348:1 (comment) len=77 "// the candidate switch happens to be the right implementor). The cases below"
// interfaces_inferred_generic_type_args.baml:349:1 (comment) len=70 "// pin the instantiation itself, so they cannot pass by dispatch luck:"
// interfaces_inferred_generic_type_args.baml:350:1 (comment) len=71 "//   • a free generic function must see its inferred `T` via reflect;"
// interfaces_inferred_generic_type_args.baml:351:1 (comment) len=64 "//   • an instance built by an inferred static ctor must carry"
// interfaces_inferred_generic_type_args.baml:352:1 (comment) len=68 "//     `class_type_args = [int]` — observable through a user-level"
// interfaces_inferred_generic_type_args.baml:353:1 (comment) len=61 "//     `match`/IsType test, which consults the args directly."
// interfaces_inferred_generic_type_args.baml:354:1 (comment) len=73 "// Both were broken before TIR began persisting inferred call bindings in"
// interfaces_inferred_generic_type_args.baml:355:1 (comment) len=57 "// `CallPlan.type_args` for MIR to seed the callee frame."
// interfaces_inferred_generic_type_args.baml:356:1 (comment) len=228 "// ═══════════════════════════════════════════════════════════════════════════"
// interfaces_inferred_generic_type_args.baml:358:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:358:10 (function) [declaration] len=13 "free_fn_tname"
// interfaces_inferred_generic_type_args.baml:358:23 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:358:24 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:358:25 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:358:27 (parameter) [declaration] len=1 "a"
// interfaces_inferred_generic_type_args.baml:358:30 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:358:38 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:359:5 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:359:12 (namespace) [defaultLibrary] len=7 "reflect"
// interfaces_inferred_generic_type_args.baml:359:19 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:359:20 (function) len=7 "type_of"
// interfaces_inferred_generic_type_args.baml:359:27 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:359:28 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:359:29 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:359:33 (method) len=9 "to_string"
// interfaces_inferred_generic_type_args.baml:362:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:362:10 (function) [declaration] len=18 "free_fn_inferred_t"
// interfaces_inferred_generic_type_args.baml:362:34 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:363:5 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:363:12 (function) len=13 "free_fn_tname"
// interfaces_inferred_generic_type_args.baml:363:27 (number) len=1 "1"
// interfaces_inferred_generic_type_args.baml:363:30 (number) len=1 "2"
// interfaces_inferred_generic_type_args.baml:363:33 (number) len=1 "3"
// interfaces_inferred_generic_type_args.baml:366:1 (keyword) len=4 "test"
// interfaces_inferred_generic_type_args.baml:366:6 (string) len=38 "\"free_function_sees_inferred_type_arg\""
// interfaces_inferred_generic_type_args.baml:367:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_inferred_generic_type_args.baml:367:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:367:12 (function) len=5 "equal"
// interfaces_inferred_generic_type_args.baml:367:18 (function) len=18 "free_fn_inferred_t"
// interfaces_inferred_generic_type_args.baml:367:40 (string) len=5 "\"int\""
// interfaces_inferred_generic_type_args.baml:370:1 (comment) len=79 "// The inferred-ctor instance must pass a runtime `IsType ArrBag<int>` test —"
// interfaces_inferred_generic_type_args.baml:371:1 (comment) len=40 "// NOT fall through to the wildcard arm."
// interfaces_inferred_generic_type_args.baml:372:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:372:10 (function) [declaration] len=31 "inferred_ctor_instance_is_typed"
// interfaces_inferred_generic_type_args.baml:372:47 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:372:51 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:372:58 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:373:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:373:9 (variable) [declaration] len=1 "s"
// interfaces_inferred_generic_type_args.baml:373:12 (interface) len=3 "Bag"
// interfaces_inferred_generic_type_args.baml:373:15 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:373:16 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:373:19 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:373:21 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:373:23 (class) len=6 "ArrBag"
// interfaces_inferred_generic_type_args.baml:373:30 (method) len=4 "make"
// interfaces_inferred_generic_type_args.baml:373:36 (number) len=1 "5"
// interfaces_inferred_generic_type_args.baml:373:39 (number) len=1 "6"
// interfaces_inferred_generic_type_args.baml:373:42 (number) len=1 "7"
// interfaces_inferred_generic_type_args.baml:374:5 (keyword) len=5 "match"
// interfaces_inferred_generic_type_args.baml:374:12 (variable) len=1 "s"
// interfaces_inferred_generic_type_args.baml:375:9 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:375:13 (variable) [declaration] len=1 "a"
// interfaces_inferred_generic_type_args.baml:375:16 (class) len=6 "ArrBag"
// interfaces_inferred_generic_type_args.baml:375:22 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:375:23 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:375:26 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:375:31 (number) len=1 "1"
// interfaces_inferred_generic_type_args.baml:376:9 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:376:13 (variable) [declaration] len=1 "o"
// interfaces_inferred_generic_type_args.baml:376:16 (class) len=6 "OneBag"
// interfaces_inferred_generic_type_args.baml:376:22 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:376:23 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:376:26 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:376:31 (number) len=1 "2"
// interfaces_inferred_generic_type_args.baml:377:14 (number) len=1 "9"
// interfaces_inferred_generic_type_args.baml:381:1 (keyword) len=4 "test"
// interfaces_inferred_generic_type_args.baml:381:6 (string) len=55 "\"inferred_static_ctor_instance_carries_class_type_args\""
// interfaces_inferred_generic_type_args.baml:382:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_inferred_generic_type_args.baml:382:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:382:12 (function) len=5 "equal"
// interfaces_inferred_generic_type_args.baml:382:18 (function) len=31 "inferred_ctor_instance_is_typed"
// interfaces_inferred_generic_type_args.baml:382:53 (number) len=1 "1"
// interfaces_inferred_generic_type_args.baml:385:1 (comment) len=76 "// Lazy-iterator shape end-to-end: inferred static ctor, consumption through"
// interfaces_inferred_generic_type_args.baml:386:1 (comment) len=74 "// an inherited default method that drives `self.next()` in a loop, with a"
// interfaces_inferred_generic_type_args.baml:387:1 (comment) len=77 "// second implementor present. Returns the COLLECTED COUNT, so a mis-dispatch"
// interfaces_inferred_generic_type_args.baml:388:1 (comment) len=71 "// (e.g. `next` resolving to `Lone.next` against an `ArrIter` receiver)"
// interfaces_inferred_generic_type_args.baml:389:1 (comment) len=30 "// yields 0 or 1 instead of 3."
// interfaces_inferred_generic_type_args.baml:390:1 (keyword) len=9 "interface"
// interfaces_inferred_generic_type_args.baml:390:11 (interface) [declaration] len=8 "NextIter"
// interfaces_inferred_generic_type_args.baml:390:19 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:390:20 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:390:23 (typeParameter) [declaration] len=1 "E"
// interfaces_inferred_generic_type_args.baml:390:24 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:391:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:391:12 (method) [declaration] len=5 "next2"
// interfaces_inferred_generic_type_args.baml:391:18 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:391:27 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:391:29 (operator) len=1 "|"
// interfaces_inferred_generic_type_args.baml:391:31 (class) len=8 "IterDone"
// interfaces_inferred_generic_type_args.baml:391:40 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:391:47 (type) len=1 "E"
// interfaces_inferred_generic_type_args.baml:393:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:393:12 (method) [declaration] len=8 "collect2"
// interfaces_inferred_generic_type_args.baml:393:21 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:393:30 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:393:34 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:393:41 (type) len=1 "E"
// interfaces_inferred_generic_type_args.baml:394:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:394:9 (variable) [declaration] len=3 "out"
// interfaces_inferred_generic_type_args.baml:394:14 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:394:18 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:395:5 (keyword) len=5 "while"
// interfaces_inferred_generic_type_args.baml:395:12 (boolean) len=4 "true"
// interfaces_inferred_generic_type_args.baml:396:7 (keyword) len=5 "match"
// interfaces_inferred_generic_type_args.baml:396:14 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:396:19 (method) len=5 "next2"
// interfaces_inferred_generic_type_args.baml:397:9 (class) len=8 "IterDone"
// interfaces_inferred_generic_type_args.baml:397:23 (keyword) len=5 "break"
// interfaces_inferred_generic_type_args.baml:398:9 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:398:13 (variable) [declaration] len=1 "x"
// interfaces_inferred_generic_type_args.baml:398:16 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:398:23 (variable) len=3 "out"
// interfaces_inferred_generic_type_args.baml:398:27 (method) len=4 "push"
// interfaces_inferred_generic_type_args.baml:398:32 (variable) len=1 "x"
// interfaces_inferred_generic_type_args.baml:401:5 (variable) len=3 "out"
// interfaces_inferred_generic_type_args.baml:405:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:405:7 (class) [declaration] len=8 "IterDone"
// interfaces_inferred_generic_type_args.baml:408:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:408:7 (class) [declaration] len=7 "ArrIter"
// interfaces_inferred_generic_type_args.baml:408:14 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:408:15 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:408:16 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:409:5 (property) [declaration] len=3 "arr"
// interfaces_inferred_generic_type_args.baml:409:10 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:410:5 (property) [declaration] len=3 "idx"
// interfaces_inferred_generic_type_args.baml:410:10 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:412:5 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:412:14 (method) [declaration] len=2 "of"
// interfaces_inferred_generic_type_args.baml:412:17 (parameter) [declaration] len=1 "a"
// interfaces_inferred_generic_type_args.baml:412:20 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:412:28 (class) len=7 "ArrIter"
// interfaces_inferred_generic_type_args.baml:412:35 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:412:36 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:412:37 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:412:39 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:412:46 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:413:9 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:413:16 (class) len=7 "ArrIter"
// interfaces_inferred_generic_type_args.baml:413:23 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:413:24 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:413:25 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:413:29 (property) len=3 "arr"
// interfaces_inferred_generic_type_args.baml:413:34 (parameter) len=1 "a"
// interfaces_inferred_generic_type_args.baml:413:37 (property) len=3 "idx"
// interfaces_inferred_generic_type_args.baml:413:42 (number) len=1 "0"
// interfaces_inferred_generic_type_args.baml:416:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:416:16 (interface) len=8 "NextIter"
// interfaces_inferred_generic_type_args.baml:416:24 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:416:25 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:416:28 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:416:33 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:417:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:417:18 (method) [declaration] len=5 "next2"
// interfaces_inferred_generic_type_args.baml:417:24 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:417:33 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:417:35 (operator) len=1 "|"
// interfaces_inferred_generic_type_args.baml:417:37 (class) len=8 "IterDone"
// interfaces_inferred_generic_type_args.baml:417:46 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:417:53 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:418:13 (keyword) len=5 "match"
// interfaces_inferred_generic_type_args.baml:418:20 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:418:25 (property) len=3 "arr"
// interfaces_inferred_generic_type_args.baml:418:29 (method) len=2 "at"
// interfaces_inferred_generic_type_args.baml:418:32 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:418:37 (property) len=3 "idx"
// interfaces_inferred_generic_type_args.baml:419:17 (type) [defaultLibrary] len=4 "null"
// interfaces_inferred_generic_type_args.baml:419:25 (class) len=8 "IterDone"
// interfaces_inferred_generic_type_args.baml:420:17 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:420:21 (variable) [declaration] len=1 "x"
// interfaces_inferred_generic_type_args.baml:420:24 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:421:21 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:421:26 (property) len=3 "idx"
// interfaces_inferred_generic_type_args.baml:421:30 (operator) len=2 "+="
// interfaces_inferred_generic_type_args.baml:421:33 (number) len=1 "1"
// interfaces_inferred_generic_type_args.baml:422:21 (variable) len=1 "x"
// interfaces_inferred_generic_type_args.baml:429:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:429:7 (class) [declaration] len=8 "LoneIter"
// interfaces_inferred_generic_type_args.baml:429:15 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:429:16 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:429:17 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:430:5 (property) [declaration] len=1 "v"
// interfaces_inferred_generic_type_args.baml:430:8 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:431:5 (property) [declaration] len=4 "used"
// interfaces_inferred_generic_type_args.baml:431:11 (type) [defaultLibrary] len=4 "bool"
// interfaces_inferred_generic_type_args.baml:433:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:433:16 (interface) len=8 "NextIter"
// interfaces_inferred_generic_type_args.baml:433:24 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:433:25 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:433:28 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:433:33 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:434:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:434:18 (method) [declaration] len=5 "next2"
// interfaces_inferred_generic_type_args.baml:434:24 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:434:33 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:434:35 (operator) len=1 "|"
// interfaces_inferred_generic_type_args.baml:434:37 (class) len=8 "IterDone"
// interfaces_inferred_generic_type_args.baml:434:46 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:434:53 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:435:13 (keyword) len=2 "if"
// interfaces_inferred_generic_type_args.baml:435:17 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:435:22 (property) len=4 "used"
// interfaces_inferred_generic_type_args.baml:436:17 (class) len=8 "IterDone"
// interfaces_inferred_generic_type_args.baml:437:15 (keyword) len=4 "else"
// interfaces_inferred_generic_type_args.baml:438:17 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:438:22 (property) len=4 "used"
// interfaces_inferred_generic_type_args.baml:438:27 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:438:29 (boolean) len=4 "true"
// interfaces_inferred_generic_type_args.baml:439:17 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:439:22 (property) len=1 "v"
// interfaces_inferred_generic_type_args.baml:445:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:445:10 (function) [declaration] len=27 "inferred_ctor_collect_count"
// interfaces_inferred_generic_type_args.baml:445:43 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:445:47 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:445:54 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:446:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:446:9 (variable) [declaration] len=2 "it"
// interfaces_inferred_generic_type_args.baml:446:13 (interface) len=8 "NextIter"
// interfaces_inferred_generic_type_args.baml:446:21 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:446:22 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:446:27 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:446:32 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:446:34 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:446:36 (class) len=7 "ArrIter"
// interfaces_inferred_generic_type_args.baml:446:44 (method) len=2 "of"
// interfaces_inferred_generic_type_args.baml:446:48 (number) len=1 "1"
// interfaces_inferred_generic_type_args.baml:446:51 (number) len=1 "2"
// interfaces_inferred_generic_type_args.baml:446:54 (number) len=1 "3"
// interfaces_inferred_generic_type_args.baml:447:5 (variable) len=2 "it"
// interfaces_inferred_generic_type_args.baml:447:8 (method) len=8 "collect2"
// interfaces_inferred_generic_type_args.baml:447:19 (method) len=6 "length"
// interfaces_inferred_generic_type_args.baml:450:1 (keyword) len=4 "test"
// interfaces_inferred_generic_type_args.baml:450:6 (string) len=43 "\"inferred_ctor_default_method_collects_all\""
// interfaces_inferred_generic_type_args.baml:451:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_inferred_generic_type_args.baml:451:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:451:12 (function) len=5 "equal"
// interfaces_inferred_generic_type_args.baml:451:18 (function) len=27 "inferred_ctor_collect_count"
// interfaces_inferred_generic_type_args.baml:451:49 (number) len=1 "3"
// interfaces_inferred_generic_type_args.baml:454:1 (comment) len=83 "// ─── Inferred binding nesting a rigid `Self` must NOT seed `void` ───"
// interfaces_inferred_generic_type_args.baml:455:1 (comment) len=2 "//"
// interfaces_inferred_generic_type_args.baml:456:1 (comment) len=79 "// Inside an interface default method, `self` has the rigid pinned type `Self`,"
// interfaces_inferred_generic_type_args.baml:457:1 (comment) len=78 "// which is never among the caller frame's generic params. An inferred generic"
// interfaces_inferred_generic_type_args.baml:458:1 (comment) len=75 "// call whose binding NESTS it (here `T := Self[]` via `echo_type([self])`)"
// interfaces_inferred_generic_type_args.baml:459:1 (comment) len=79 "// cannot be resolved to a frame index — the seeding must demote that binding"
// interfaces_inferred_generic_type_args.baml:460:1 (comment) len=70 "// to `unknown` (the unseeded status quo), not lower `Self` to `void`."
// interfaces_inferred_generic_type_args.baml:462:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:462:10 (function) [declaration] len=9 "echo_type"
// interfaces_inferred_generic_type_args.baml:462:19 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:462:20 (typeParameter) [declaration] len=1 "T"
// interfaces_inferred_generic_type_args.baml:462:21 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:462:23 (parameter) [declaration] len=1 "a"
// interfaces_inferred_generic_type_args.baml:462:26 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:462:32 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:463:5 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:463:12 (namespace) [defaultLibrary] len=7 "reflect"
// interfaces_inferred_generic_type_args.baml:463:19 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:463:20 (function) len=7 "type_of"
// interfaces_inferred_generic_type_args.baml:463:27 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:463:28 (type) len=1 "T"
// interfaces_inferred_generic_type_args.baml:463:29 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:463:33 (method) len=9 "to_string"
// interfaces_inferred_generic_type_args.baml:466:1 (keyword) len=9 "interface"
// interfaces_inferred_generic_type_args.baml:466:11 (interface) [declaration] len=8 "SelfEcho"
// interfaces_inferred_generic_type_args.baml:467:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:467:12 (method) [declaration] len=3 "tag"
// interfaces_inferred_generic_type_args.baml:467:16 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:467:25 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:467:32 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:467:39 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:468:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:468:12 (method) [declaration] len=16 "nested_self_type"
// interfaces_inferred_generic_type_args.baml:468:29 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:468:38 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:468:45 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:468:52 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:469:5 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:469:12 (function) len=9 "echo_type"
// interfaces_inferred_generic_type_args.baml:469:23 (parameter) len=4 "self"
// interfaces_inferred_generic_type_args.baml:473:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:473:7 (class) [declaration] len=5 "EchoA"
// interfaces_inferred_generic_type_args.baml:474:5 (property) [declaration] len=1 "n"
// interfaces_inferred_generic_type_args.baml:474:8 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:475:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:475:16 (interface) len=8 "SelfEcho"
// interfaces_inferred_generic_type_args.baml:476:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:476:18 (method) [declaration] len=3 "tag"
// interfaces_inferred_generic_type_args.baml:476:22 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:476:31 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:477:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:477:20 (string) len=3 "\"a\""
// interfaces_inferred_generic_type_args.baml:482:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:482:7 (class) [declaration] len=5 "EchoB"
// interfaces_inferred_generic_type_args.baml:483:5 (property) [declaration] len=1 "s"
// interfaces_inferred_generic_type_args.baml:483:8 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:484:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:484:16 (interface) len=8 "SelfEcho"
// interfaces_inferred_generic_type_args.baml:485:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:485:18 (method) [declaration] len=3 "tag"
// interfaces_inferred_generic_type_args.baml:485:22 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:485:31 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:486:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:486:20 (string) len=3 "\"b\""
// interfaces_inferred_generic_type_args.baml:491:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:491:10 (function) [declaration] len=28 "nested_self_inferred_binding"
// interfaces_inferred_generic_type_args.baml:491:44 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:491:51 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:491:58 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:492:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:492:9 (variable) [declaration] len=1 "e"
// interfaces_inferred_generic_type_args.baml:492:12 (interface) len=8 "SelfEcho"
// interfaces_inferred_generic_type_args.baml:492:21 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:492:23 (class) len=5 "EchoA"
// interfaces_inferred_generic_type_args.baml:492:31 (property) len=1 "n"
// interfaces_inferred_generic_type_args.baml:492:34 (number) len=1 "1"
// interfaces_inferred_generic_type_args.baml:493:5 (variable) len=1 "e"
// interfaces_inferred_generic_type_args.baml:493:7 (method) len=16 "nested_self_type"
// interfaces_inferred_generic_type_args.baml:496:1 (comment) len=78 "// `unknown` (the whole binding demoted) is the pre-existing status quo for an"
// interfaces_inferred_generic_type_args.baml:497:1 (comment) len=81 "// unresolvable binding — the regression this guards against is `void[]`, which"
// interfaces_inferred_generic_type_args.baml:498:1 (comment) len=56 "// a naive lowering of the nested rigid `Self` produces."
// interfaces_inferred_generic_type_args.baml:499:1 (keyword) len=4 "test"
// interfaces_inferred_generic_type_args.baml:499:6 (string) len=41 "\"nested_self_inferred_binding_not_voided\""
// interfaces_inferred_generic_type_args.baml:500:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_inferred_generic_type_args.baml:500:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:500:12 (function) len=5 "equal"
// interfaces_inferred_generic_type_args.baml:500:18 (function) len=28 "nested_self_inferred_binding"
// interfaces_inferred_generic_type_args.baml:500:50 (string) len=9 "\"unknown\""
// interfaces_inferred_generic_type_args.baml:503:1 (comment) len=74 "// Interface dispatch must also carry inferred method-level type args. The"
// interfaces_inferred_generic_type_args.baml:504:1 (comment) len=79 "// dispatch arm itself seeds the receiver/interface frame; `CallPlan.type_args`"
// interfaces_inferred_generic_type_args.baml:505:1 (comment) len=38 "// supplies the method's inferred `U`."
// interfaces_inferred_generic_type_args.baml:506:1 (keyword) len=9 "interface"
// interfaces_inferred_generic_type_args.baml:506:11 (interface) [declaration] len=9 "TypeProbe"
// interfaces_inferred_generic_type_args.baml:507:3 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:507:12 (method) [declaration] len=4 "seen"
// interfaces_inferred_generic_type_args.baml:507:16 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:507:17 (typeParameter) [declaration] len=1 "U"
// interfaces_inferred_generic_type_args.baml:507:18 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:507:20 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:507:26 (parameter) [declaration] len=5 "value"
// interfaces_inferred_generic_type_args.baml:507:33 (type) len=1 "U"
// interfaces_inferred_generic_type_args.baml:507:39 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:507:46 (keyword) len=6 "throws"
// interfaces_inferred_generic_type_args.baml:507:53 (type) [defaultLibrary] len=5 "never"
// interfaces_inferred_generic_type_args.baml:510:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:510:7 (class) [declaration] len=6 "ProbeA"
// interfaces_inferred_generic_type_args.baml:511:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:511:16 (interface) len=9 "TypeProbe"
// interfaces_inferred_generic_type_args.baml:512:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:512:18 (method) [declaration] len=4 "seen"
// interfaces_inferred_generic_type_args.baml:512:22 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:512:23 (typeParameter) [declaration] len=1 "U"
// interfaces_inferred_generic_type_args.baml:512:24 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:512:26 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:512:32 (parameter) [declaration] len=5 "value"
// interfaces_inferred_generic_type_args.baml:512:39 (type) len=1 "U"
// interfaces_inferred_generic_type_args.baml:512:45 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:513:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:513:20 (namespace) [defaultLibrary] len=7 "reflect"
// interfaces_inferred_generic_type_args.baml:513:27 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:513:28 (function) len=7 "type_of"
// interfaces_inferred_generic_type_args.baml:513:35 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:513:36 (type) len=1 "U"
// interfaces_inferred_generic_type_args.baml:513:37 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:513:41 (method) len=9 "to_string"
// interfaces_inferred_generic_type_args.baml:518:1 (keyword) len=5 "class"
// interfaces_inferred_generic_type_args.baml:518:7 (class) [declaration] len=6 "ProbeB"
// interfaces_inferred_generic_type_args.baml:519:5 (keyword) len=10 "implements"
// interfaces_inferred_generic_type_args.baml:519:16 (interface) len=9 "TypeProbe"
// interfaces_inferred_generic_type_args.baml:520:9 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:520:18 (method) [declaration] len=4 "seen"
// interfaces_inferred_generic_type_args.baml:520:22 (operator) len=1 "<"
// interfaces_inferred_generic_type_args.baml:520:23 (typeParameter) [declaration] len=1 "U"
// interfaces_inferred_generic_type_args.baml:520:24 (operator) len=1 ">"
// interfaces_inferred_generic_type_args.baml:520:26 (parameter) [declaration] len=4 "self"
// interfaces_inferred_generic_type_args.baml:520:32 (parameter) [declaration] len=5 "value"
// interfaces_inferred_generic_type_args.baml:520:39 (type) len=1 "U"
// interfaces_inferred_generic_type_args.baml:520:45 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:521:13 (keyword) len=6 "return"
// interfaces_inferred_generic_type_args.baml:521:20 (string) len=7 "\"wrong\""
// interfaces_inferred_generic_type_args.baml:526:1 (keyword) len=8 "function"
// interfaces_inferred_generic_type_args.baml:526:10 (function) [declaration] len=43 "interface_dispatch_inferred_method_type_arg"
// interfaces_inferred_generic_type_args.baml:526:59 (type) [defaultLibrary] len=6 "string"
// interfaces_inferred_generic_type_args.baml:527:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:527:9 (variable) [declaration] len=1 "p"
// interfaces_inferred_generic_type_args.baml:527:12 (interface) len=9 "TypeProbe"
// interfaces_inferred_generic_type_args.baml:527:22 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:527:24 (class) len=6 "ProbeA"
// interfaces_inferred_generic_type_args.baml:528:5 (keyword) len=3 "let"
// interfaces_inferred_generic_type_args.baml:528:9 (variable) [declaration] len=1 "x"
// interfaces_inferred_generic_type_args.baml:528:12 (type) [defaultLibrary] len=3 "int"
// interfaces_inferred_generic_type_args.baml:528:16 (operator) len=1 "="
// interfaces_inferred_generic_type_args.baml:528:18 (number) len=1 "1"
// interfaces_inferred_generic_type_args.baml:529:5 (variable) len=1 "p"
// interfaces_inferred_generic_type_args.baml:529:7 (method) len=4 "seen"
// interfaces_inferred_generic_type_args.baml:529:12 (variable) len=1 "x"
// interfaces_inferred_generic_type_args.baml:532:1 (keyword) len=4 "test"
// interfaces_inferred_generic_type_args.baml:532:6 (string) len=50 "\"interface_dispatch_sees_inferred_method_type_arg\""
// interfaces_inferred_generic_type_args.baml:533:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_inferred_generic_type_args.baml:533:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_inferred_generic_type_args.baml:533:12 (function) len=5 "equal"
// interfaces_inferred_generic_type_args.baml:533:18 (function) len=43 "interface_dispatch_inferred_method_type_arg"
// interfaces_inferred_generic_type_args.baml:533:65 (string) len=5 "\"int\""
