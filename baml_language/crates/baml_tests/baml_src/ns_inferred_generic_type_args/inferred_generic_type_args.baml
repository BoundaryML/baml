// End-to-end tests for the generic-type-arg dispatch fix.
//
// A generic class whose `<T>` is *inferred* (rather than written explicitly)
// must carry the correct runtime `class_type_args`, and a method dispatched
// through a multi-implementor interface must receive the resolved
// class/interface type args in its frame. Without this, a method body that
// reads its enclosing `T` at runtime — `reflect.Type.of<T>()`, or constructing
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
    return reflect.Type.of<T>().to_string()
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
    return reflect.Type.of<T>().to_string()
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
// must resolve its class `T` (here via `reflect.Type.of<T>()`).

interface Describable {
  function describe(self) -> string throws never
}

class TypedBox<T> {
    value: T,
    implements Describable {
        function describe(self) -> string {
            return reflect.Type.of<T>().to_string();
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
    return reflect.Type.of<T>().to_string();
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

// ─── Inferred binding nesting the rigid `Self` resolves through its slot ───
//
// Inside an interface default method, `self` has the rigid pinned type `Self`,
// carried at frame slot 0. An inferred generic call whose binding NESTS it
// (here `T := Self[]` via `echo_type([self])`) seeds the callee with the
// realized receiver type — the reflected result is the concrete implementor's
// array type, never a `void[]` (the naive-lowering regression this guarded
// against) and no longer the demoted `unknown` of the slotless era.

function echo_type<T>(a: T) -> string {
    return reflect.Type.of<T>().to_string();
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

test "nested_self_inferred_binding_not_voided" {
    assert.equal(nested_self_inferred_binding(), "inferred_generic_type_args.EchoA[]")
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
            return reflect.Type.of<U>().to_string();
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
