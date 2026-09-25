# Error-origin review: landing-note lifetimes

A review of the [error-origin implementation](btel-query-source-errors.md)
found that a catch's landing note kept proving things after the catch had
finished. Real recordings confirmed one false proven link and one lost
link. Both are fixed on the exception and conversion paths only.

A second probe found a false proven conversion inside a catch that was still
running. Landing notes cannot fix that one, so an `UnknownError` conversion
now never establishes a proven link. See
[A conversion inside a running catch](#a-conversion-inside-a-running-catch).

## What went wrong

When unwinding lands in a handler, the VM notes which raise put the error
into which stack slot. A later rethrow looks for notes whose slot still
holds the value. Until the second fix below, an `UnknownError` conversion
did too. Before the fix, a note
stayed usable for as long as its frame existed. Nothing checked that its
handler was still running. A finished catch leaves both its note and its
slot behind.

### A false proven link

```baml
function Word() -> string { "same" }
function Same() -> int throws string { throw Word() }

function convert_unrelated(n: int) -> int {
    let a = Same() catch (e) { _ => 1 };   // raise 1 lands; the catch finishes
    let x = Word();                         // the same interned "same"
    let r = { throw baml.errors.UnknownError.from<int>(x) } catch (outer) { _ => 7 };
    a + r
}
```

`x` was never raised or caught. It is only the same interned string object
that the finished catch's slot still held. The conversion was recorded as
**proven** to raise 1's occurrence. The regression test failed like this
before the fix:

```text
test a_conversion_of_an_unrelated_equal_value_is_not_proven ... FAILED
  left: Some("6da03a60fb6548b3afd1edaf13889008:4115")
 right: Some("6da03a60fb6548b3afd1edaf13889008:4115")
(the converted raise's occurrence equals the first raise)
```

The VM's own diagnostic trace has the same weakness. Its throw contexts are
keyed by value, so it hands raise 1's trace to the converted throw. The
telemetry still records that trace, but only as a weaker "inherited" trace,
never as a proven origin.

### A lost link

```baml
function sequential(n: int) -> int {
    let a = Same() catch (e) { _ => 1 };
    let b = { Same() catch (e2) { _ => { throw e2 } } } catch (outer) { _ => 2 };
    a + b
}
```

Both catches caught `"same"`. The rethrow of `e2` found the finished first
catch's note as well as its own, so it became `ambiguous` with two
candidates instead of proven:

```text
test a_finished_handler_does_not_make_a_later_rethrow_ambiguous ... FAILED
  left: ("ambiguous", None)
 right: ("proven", Some("752db02e5cc74a8d88e8eb133525d276:12291"))
```

## The fix

A note now counts only while its handler is running.

- **Active handlers.** For each live frame, the lookup takes the handlers
  whose body covers the frame's current PC. For the raise frame that is the
  live PC; for outer frames it is the call site, as the unwinder uses them.
  The compiler already emits these ranges in the handler-context table, one
  entry per block of a catch's dispatch and arms, or of a defer pad's body.
  Each entry uses the same handler PC as the exception table.
- **Superseded notes.** A note is retired by any later landing in the same
  frame depth and function whose handler does not start inside its body.
  That covers a sibling catch, an enclosing catch, and the same frame reused
  by a later call. A catch nested inside a running handler does not retire
  its parent, so two truly live handlers holding one object still come out
  `ambiguous`.
- **Unaccounted handlers.** If a handler is running, has no valid note, and
  its error slot holds the value, the answer is `unresolved`. That covers an
  error forwarded into a handler without landing there, and a stale note that
  was retired.
- **Slot writes.** If any instruction in a handler's body stores into its
  error slot (`StoreVar`, `StoreVarLoadVar`, `StoreVar2`, `NarrowBind`), its
  note cannot vouch for what the slot holds. The answer is `unresolved`. This
  is the conservative answer for a reassigned catch binding.

Everything else is unchanged: one proven origin, `ambiguous` with a count,
or `unresolved`. Notes still hold stack indexes and IDs only. Each note now
also carries a landing sequence number.

With the fix, `convert_unrelated`'s conversion is `unresolved` via
`normalization`, and `sequential`'s rethrow is proven to the second raise.

## A conversion inside a running catch

The lifetime rules still let a conversion match a catch that is running:

```baml
function convert_in_live_catch(n: int) -> int {
    let r = {
        Same() catch (e) { _ => { let x = Word(); throw baml.errors.UnknownError.from<int>(x) } }
    } catch (outer) { _ => 9 };
    r
}
```

The catch is running and its slot holds the interned `"same"`. `x` comes
from a separate `Word()` call, but it is the same interned string. The
conversion was recorded as **proven** to the `Same()` raise:

```text
test a_conversion_inside_a_live_catch_of_an_unrelated_equal_value_is_not_proven ... FAILED
assertion `left != right` failed: the conversion of an unrelated value was attributed to the live catch's occurrence: Raise {
    origin: "proven",
  left: Some("79f3059c2faf4a649fc752dfdff522f9:8201")
 right: Some("79f3059c2faf4a649fc752dfdff522f9:8201")
```

A note cannot fix this. `UnknownError.from` reaches the VM through ordinary
BAML helper frames, and the VM only receives the source value. A running
catch whose slot holds an equal value does not show that the conversion
read that slot. An int, or any interned value, can match the same way.

The fix: an `UnknownError` conversion never establishes a proven link.
When the converted value's throw carries a context kept from an earlier
throw of an equal value, its raise is `unresolved` via `normalization`,
with the new reason `source_not_recorded` (wire value 6). The VM still hands
that diagnostic trace to the converted throw, and the recording keeps it as
an inherited trace. That includes `from(e)`, which was proven before. A
conversion of a value never thrown before carries no context and stays a
fresh raise, as `convert_control` shows. This is a known gap, not a
proof. Rethrows and awaits keep their rules.

What changed:

- `bex_vm/src/vm/error_evidence.rs`: a throw that consumes a preserved
  conversion context is `unresolved`/`source_not_recorded`. `held_value_origin` and
  `conversion_origin` are removed.
- `bex_vm/src/vm.rs`: `ThrowContext` loses the `origin` field.
  `preserve_throw_context` is back to HEAD.
- `btel_records`, `btel_recorder` (proto and mapping), and the
  `baml_query_btel` catalog gain the reason `source_not_recorded`. The
  catalog's help text no longer lists conversions among proven links.
- `error_origin_lifetimes`: new test
  `a_conversion_inside_a_live_catch_of_an_unrelated_equal_value_is_not_proven`.
  It failed as shown above before the fix and passes now.
- `error_origins`: the `converted` case (`from(e)` in a catch arm) now
  expects `unresolved`, `source_not_recorded` and a complete inherited
  trace. It expected `proven` before.

Validation, under the same lock and settings:
`cargo +1.98.0 test -j 4 -p baml_query_btel -p btel_reader -p btel_recorder -p btel_records -p btel_file`
(every target), and `cargo +1.98.0 test -j 4 -p bex_vm --lib`: all pass.
Clippy with warnings denied on `bex_vm`, `btel_records`, `btel_recorder` and
`baml_query_btel` (all targets), and rustfmt with the CI import options:
clean. Raw failing output and the exact patch are in
`baml_language/target/throw-perf/origin-probe/`.

## What changed

- `bex_vm/src/telemetry/errors.rs`: notes carry a landing sequence.
  `ErrorBook::lookup` takes per-frame scopes (depth, function, active
  handlers, a body test) and applies the rules above. `land()` keeps its
  signature and behavior. The unit tests are rewritten for the new rules.
- `bex_vm/src/vm/error_evidence.rs`: `rethrow_origin` and
  `held_value_origin` build those scopes from each frame's compact tables.
  (`held_value_origin` was removed later; see the section above.)
  New helpers are `handler_frame`, `landing_origin`, `active_handlers`,
  `in_handler_body` and `handler_writes_local`. No other method changed.
- `bex_vm/src/telemetry.rs`: re-exports `ActiveHandler` and `FrameScope`.
- `baml_query_btel/tests/error_origin_lifetimes.rs`: new real-recording
  regressions.

No successful call, return, await or instruction path changed. No frame,
function, future or record layout changed. The new work runs only when a
rethrow's origin is looked up, with a recording on. (A conversion's origin
was looked up this way too, until the section above removed that.) It decodes the frame's handler tables and scans each running handler's
body for stores. It was not timed: throw-heavy performance is being
measured separately.

## Tests

From `baml_language/`, using the pinned Rust 1.98.0 toolchain, with
`RUSTC_WRAPPER=`, `RUSTC_WORKSPACE_WRAPPER=` and `CARGO_INCREMENTAL=0`:

```sh
cargo +1.98.0 test -j 4 -p bex_vm -p baml_query_btel
cargo +1.98.0 clippy -j 4 -p bex_vm -p baml_query_btel -p bex_engine --all-targets -- -D warnings
```

Builds, tests and measurements were serialized through
`flock /tmp/btel-source-errors-validation.lock`. All pass after the fix:

- `error_origin_lifetimes`:
  - `a_conversion_of_an_unrelated_equal_value_is_not_proven`: failed before
    the fix, passes now.
  - `a_finished_handler_does_not_make_a_later_rethrow_ambiguous`: failed
    before the fix, passes now.
  - `a_second_invocation_never_inherits_the_first_ones_notes`: the same
    function called twice at one frame depth, with a plain throw and with a
    conversion, plus a control run with no earlier catch.
- `error_origins`, unchanged by the lifetime fix. The rethrow is still
  proven, nested handlers holding one object are still `ambiguous` (2),
  defer is still proven, and a GC between catch and rethrow keeps the
  origin. The `UnknownError` conversion inside a catch arm stayed proven
  under the lifetime fix; the section above makes it `unresolved`. Repeated awaits, equal-valued children and
  interleaved deep unwinds are unchanged.
- `bex_vm` unit tests: live nested handlers, a finished sibling, a later
  landing retiring a stale note (the defer-pad case), a handler that writes
  its error slot, one origin across frames, eviction.
- The whole `bex_vm` and `baml_query_btel` suites, Clippy with warnings
  denied on `bex_vm`, `baml_query_btel` and `bex_engine`, and rustfmt.

## Limits

- **Forwarded errors.** A defer pad that forwards the error into an
  enclosing catch in the same function makes that catch run without
  landing. A rethrow there is now `unresolved`; before, it matched the pad's
  note. That was sometimes right and sometimes stale.
- **Slot stores.** A handler whose body stores into its own error slot, even
  the same value, never proves origins. None of the tested catch shapes do
  this.
- **Layout.** The body test trusts the compiler's handler-context ranges.
  If a nested catch's entry block were laid out outside its parent's body,
  the parent's note would be retired and the answer would be `unresolved`,
  never a false proof.
- **Reassigned bindings.** Probes found that in a catch whose arm assigns
  the catch binding and then throws, the catch does not run at all. Only one
  raise is recorded, and the enclosing catch receives the original value.
  This happened with the throw nested directly and with the raise in a
  called function. Assignment without a throw does catch. So the rethrow of
  an overwritten binding cannot be reached today, and this change does not
  touch that compiler routing. If it becomes reachable, the slot-write rule
  makes such a rethrow `unresolved`. A binding in its own slot never matches
  the new value, so that is `unresolved` too. A two-clause catch with a
  typed binding does catch, and throwing that binding is recorded as a fresh
  raise of the new value.
- **The VM's own trace.** It is still keyed by value, so a converted
  unrelated value gets the earlier throw's diagnostic trace. Telemetry
  records it as an inherited trace, not an origin.
- **Conversions.** An `UnknownError` conversion never establishes a proven
  link, even `from(e)`. Linking it would need a record of which value
  the conversion received.
