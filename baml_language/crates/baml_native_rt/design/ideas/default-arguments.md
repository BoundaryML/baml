# Defaulted arguments and call layouts

Status: built, as of `8362bb03c7`, with the differences listed in "As built" at the end. Sections 1 to 8 are the proposal as it was written; its assembly is written by hand to show the shape and is not compiler output. Emitted code is in `../calls.md`.

## 1. What the frontend gives us

Defaults are evaluated in the callee, on every call. The caller passes `const <omitted>` in each empty slot and the callee's prologue tests for it:

```
fn user.a1(a: int, b: int) -> int {
    bb0: { _3 = copy _2 == const <omitted>; branch copy _3 -> [bb1, bb2]; }
    bb1: { _2 = call const fn user.next_id(copy _1) -> [bb2]; }
    bb2: { ... }
}
```

Every call terminator carries `argument_layout: Option<CallLayout>` (`baml_compiler2_mir/src/ir.rs:606`): one entry per pushed operand, `None` for a required slot, `Some(name)` for an optional one. `CallLayout::map_to` (`baml_type/src/call_layout.rs:86`) pairs required slots by position and optional slots by name. The VM does this at run time (`remap_call_arguments`, `bex_vm/src/vm.rs:7688`). An earlier design wrapped the function in a synthesized closure where it was converted; it was removed (`a748b3a694`) because conversions happen in places no expression covers, such as interface dispatch and a value narrowed from `unknown`.

Rules the checker enforces, confirmed by running it:

- A defaulted parameter is passed by name, and required parameters come first.
- A function type can carry optional parameters, written `(int, y?: int) -> int throws never`.
- Subtyping goes one way. A function with more optionals is a subtype of one with fewer; optionals match by name, so any subset can be dropped. The reverse is an error, and so is treating an optional as required.
- Two functions whose optionals differ do not join to one function type.

```baml
function f(a: int, extra: int = 2) -> int throws never { ... }
function h(a: int) -> int throws never { ... }

let g: (int) -> int throws never = f;                 // ok: extra gets its default
let g: (int, extra?: int) -> int throws never = h;    // error: mismatched types
let g: (int, int) -> int throws never = f;            // error: mismatched types
```

```baml
function foo(a: int, x: int = 1, y: int = 2) -> int throws never { ... }
function bar(a: int, y: int = 3, z: int = 4) -> int throws never { ... }

let fs = [foo, bar];
fs[0](1)   // error: a union of two function types is not a function

let fs: ((int, y?: int) -> int throws never)[] = [foo, bar];   // the shared subset: fine
fs[0](1, y = 7)   // MIR: call copy _3(const 1_i64, const 7_i64)
fs[1](1)          // MIR: call copy _7(const 1_i64, const <omitted>)
```

So a call site can only name parameters its callee really has, and a site may not know how many parameters its callee has: at `fs[0](1, y = 7)` the 7 is `foo`'s third slot and `bar`'s second.

## 2. Omitted is a tag

An argument is a tag and a payload. A parameter left to its default is passed with the tag `omitted` and any payload. One rule for every type.

```asm
; f(a: int, b: int = 2, name: string = "x"):  f(1)
  mov  x2, #INT              ; a
  mov  x3, #1
  mov  x4, #OMITTED          ; b, payload not written
  mov  x6, #OMITTED          ; name
  bl   user.f

user.f:
  cmp  w4, #OMITTED
  b.ne have_b
  mov  x4, #INT              ; b = 2
  mov  x5, #2
have_b:
  cmp  w6, #OMITTED
  b.ne have_name
  ...                        ; name = "x"
have_name:
```

MIR's `_p == const <omitted>` is a compare of the tag. A default that is a call runs in the prologue after the stack check, as ordinary MIR, on every call that omits it.

`omitted` never reaches memory. Measured over the optimized MIR of the standard library, the test corpora, the conformance cases and the speedtest programs (14,246 functions, 3,338 uses of `const <omitted>`): it appears as a call operand (1,771), in a test on a parameter (1,005) or on a local the inliner copied a parameter into (538), as a `log` intrinsic's own optional operand (16), stored into a local and overwritten at once (5), and in a test through a captured parameter's cell (3). Every tested local is assigned only from a copy, a constant or `const <omitted>`; none is loaded from a field, an array or a map. A local holding it is just a pair whose tag is `omitted`, so none of these needs anything special.

## 3. Direct calls

The callee is known, so the site's layout is mapped onto the callee's at compile time with `map_to`. No thunk, no check. This also covers the calls the optimizer leaves with fewer operands than parameters (`call const fn user.f(const 2_i64)` to a two-parameter function): the missing slots are passed as `omitted`.

## 4. Calls through a function value

The site may not know how many parameters the callee has. It pushes only its own layout, and code that knows both layouts closes the gap.

A call carries two separate pieces of information, and they answer different questions:

- Which of the slots the site knows were passed. That is in the arguments themselves: a slot left out has the tag `omitted`. (An earlier draft of this design carried it as a separate mask word, one bit per slot. With a tag on every argument the mask is the tags.)
- Which slots the site has at all: its layout. That is a compile-time constant of the site, from `argument_layout` on the terminator. The callee's layout is a property of the function.

The second is what an indirect call cannot see, so it is fused into the word the caller already tests. Every function value carries its function's layout id in the payload field of its header (`../memory.md`, "Tags"), and a value's tag is its object's header. So the caller holds the callee's type id and layout id in one register, and the check that the layouts are exactly the same is one compare of that word with a constant, with no load:

```asm
; x2 = the function value's tag, x3 = its address
  cmp  x2, #(CLOSURE | L_SITE << 32)   ; a tag in a register has no collector or busy bit (they are bits 59..63 of a header)
  b.ne remap                  ; cold: another layout, or another kind of callable
  ...                         ; load the code address, call
resume:
  ...
remap:                        ; one compare per layout that can arrive here
  ubfx x9, x2, #32, #27
  cmp  w9, #L_FOO
  b.eq via_foo
  cmp  w9, #L_BAR
  b.eq via_bar
  udf                         ; checked builds report the two ids
via_foo:
  bl   remap$L_FOO$from$L_SITE
  b    resume
```

Exactly the same is the common case and takes the fast path. Only a real difference goes to a remap thunk. A remap thunk is per pair of layouts. It takes the site's arguments, lays them out as the callee expects with `omitted` in every slot the site does not have, and calls through the same function value. For section 1's `fs[0](1, y = 7)`, site layout `(a, y?)`:

```asm
; in:  a = (x2, x3), y = (x4, x5), callee value = (x6, x7)
remap$L_FOO$from$L_SITE:      ; foo is (a, x?, y?)
  stp  x29, x30, [sp, #-16]!
  mov  x29, sp
  ...                         ; a stays in slot 0
  ...                         ; slot 1 (x) = (OMITTED, _)
  ...                         ; slot 2 (y) = the site's y, tag and payload as given, omitted or not
  ...                         ; load foo's code from the function value, call
  ldp  x29, x30, [sp], #16
  ret

remap$L_BAR$from$L_SITE:      ; bar is (a, y?, z?)
  ...                         ; slot 1 (y) = the site's y, slot 2 (z) = (OMITTED, _)
```

The callee then computes its own defaults for what it did not get, per call. A thunk is an ordinary compiled function with an ordinary call and return (the collector's stack walk assumes no tail calls), a frame record, and nothing live across its call but the result. It is marked so a traceback skips it. The function value is never replaced, so `==` on functions and a later type test see the real function. The exact registers depend on the two-word calling convention, which is not fixed yet.

A difference in argument or result types alone (`w(x: int | string)` used as `(int) -> int`) needs none of this: an `(INT, 1)` is already a valid `int | string`.

## 5. Interface dispatch

The method table is built at compile time. A row whose implementation has the interface method's layout holds the implementation. A row whose implementation has extra or reordered optionals holds a thunk with the interface method's layout that calls the implementation directly, passing `omitted` for the extras:

```baml
interface Sh { function area(self, scale: int) -> int throws never }
class Sq { s: int
  implements Sh { function area(self, scale: int, pad: int = 0) -> int throws never { ... } } }
```

Nothing is checked at the call.

## 6. Who pays

- A function with no defaults, and any direct call: nothing.
- A function with defaults: one tag compare per defaulted parameter.
- An indirect call site: the layout check (three instructions, no load), only if some function value in the program has a different layout that is compatible with the site's (same required slots, the site's optionals among its own). The program is compiled whole, so this is known. The check is at the site, so once such a function exists, calls from that site pay it even when the callee is a plain lambda.

```baml
function foo(a: int, x: int = 1) -> int throws never { ... }
let g: (int) -> int throws never = foo;            // foo escapes as a value
function apply(f: (int) -> int throws never) -> int { f(5) }
apply((n: int) -> int { n + 1 })                   // this site now checks too
```

Two things would widen this: a standard library function with defaults taken as a value, and code loaded at run time, where the compiler can no longer prove a site safe.

## 7. To verify before building

1. How many remap thunks and method-table thunks the corpus needs, how many indirect sites keep the check, and whether any standard library function with defaults is taken as a value.
2. `argument_layout: None` on synthesized calls: confirm it always means "the callee's own layout".
3. That a call through the wrong layout cannot happen silently. If arguments past the registers go on the stack and the callee pops them, a mismatch corrupts `sp`. Checked builds compare the layout id at the entry of every function-value copy.
4. Rust builtins that call a closure have no compile-time site. They need the pairs as a data table and a runtime helper. Not needed until builtins can call closures.
5. The task shim calls its body with the layout `()`, and the `scope.call_baml` hooks call a known target. Confirm both fit sections 3 and 4 unchanged.

## 8. Not covered

- A type test on a function value (`v is (int) -> int` on an `unknown`). It needs each function's type, with parameter modes, reachable from the value.
- Removing the VM's run-time mapping.

## As built

- An omitted argument is `(OMITTED, 0)`, tag 4, for every type. A local that may hold one is kept as a pair whatever its type. A captured parameter that may be omitted is a zeroed cell and a one-byte flag beside it, so `OMITTED` is never stored to an object; the collector of a checked build ends the program on one it finds in an object slot or a runtime root.
- MIR passes a constant default at the call site itself, so `OMITTED` arises only for a default that is not a constant.
- Direct calls are mapped at compile time (section 3), including calls with fewer operands than parameters.
- Every indirect site checks, not only the sites section 6 names: the whole-program rule for skipping the check is not built. The check compares the layout field of the tag alone (`lsr #32`, `cmp`), not the whole word as in section 4, so a bound method of the right layout takes the fast path. It is 3 instructions where a remap can follow and 5 where the cold arm is only a trap (`../calls.md`).
- Layout ids count from 1, so a callable nothing gave a layout fails every check.
- Remap thunks and method-table thunks are built as sections 4 and 5 describe; a thunk has no name and a traceback has no frame for it. The interface method's layout is taken from the dispatching sites' `argument_layout`. An interface method and a lambda cannot declare a default, so only declared functions and methods have named slots.
- Code compiled with checks passes the site's layout id after the closure word, and every entry reached through a value ends the program when it is not its own (`CompileOptions::checks`). No committed test makes this assertion, or the collector's `OMITTED` assertion, fire; the entry assertion was made to fire once by hand.
- The task shim checks its closure against the layout `()` like any indirect site. `scope.call_baml` hooks call a known target whose signature is pinned to one or two required values.
- Over the conformance programs at `8362bb03c7`: 20 remap thunks in 9 programs, 10 method-table thunks in 2, all in the cases written for them. One standard library function with defaults taken as a value was found by a rough scan (`ai.internal.make_role`), at a type with its own slots.

Not built: a native or an intrinsic taking an omitted operand (`log`'s optional operand among them); a class's own `to_json`, `from_json` or `eq` with extra defaulted parameters (refused when checked); a type test on a function value; removing the VM's run-time mapping; the data table of layout pairs a native that calls closures would need.
