// Spike tests for the `Sortable` array-sort design, now built on
// `baml.ops.Compare`
// (thoughts/sam-projects/array-sort/01b-option-5-tdd-plan.md and the
// 02-native-sort-fast-path follow-up). The associated-error machinery the
// original `Comparable` carried is exercised here by the throwaway `HasErr` /
// `Wrap` interfaces below — `Compare.cmp` is `throws never`, so the stdlib has
// no such binding of its own any more.
//
// Helper functions throughout are needed because test-block locals are boxed
// (VM quirk; see ns_arrays/sort_compare.baml's header note).
//
// Tests requiring compiler-diagnostics assertions (setup_test_db + collect_diagnostics,
// checking specific compile errors or isolated-project compiles) cannot be
// expressed as BAML corpus tests:
//   - spike_1a_projection_valued_assoc_binding_in_blanket_impl_compiles
//   - spike_1a_projection_with_concrete_error_requires_handling
//   - spike_1a_symbolic_projection_in_generic_signature_compiles
//   - spike_1a_diagnostics_path_survives_non_qualifying_projection
//   - spike_1b_non_qualifying_element_is_compile_error_at_callsite
//   - spike_1c_interface_typed_values_cannot_call_two_self_method
//   - phase3_never_binding_callsite_normalizes_to_never
//   - phase3_error_binding_callsite_throws_it
//   - phase3_error_binding_unhandled_is_compile_error
//   - phase3_out_of_body_impl_on_builtin_normalizes_to_never
//   - phase3_out_of_body_impl_on_builtin_with_error_throws_it
//   - phase3_defaulted_assoc_override_satisfies_bare_bound
//   - phase5_union_int_float_sort_is_compile_error
//   - phase5_optional_element_sort_is_compile_error
//   - phase5_sort_by_key_nullable_key_is_compile_error
//   - phase5_union_int_string_sort_is_compile_error
//   - phase5_class_without_compare_sort_is_compile_error
//   - match_dispatch_array_type_arms_do_not_discriminate
//   - element_is_dispatch_compiles
//   - element_is_dispatch_int_callsite_normalizes_to_never
//   - element_is_dispatch_float_callsite_normalizes_to_never
//   - element_is_dispatch_user_error_callsite_requires_handling
//   - phase6_sort_error_message_names_the_array_type
//
// Opt-level-specific test (requires OptLevel::Two pinning via baml_test_optimized!):
//   - phase2_user_class_compare_direct_call_mir_optimized
//
// Wall-clock perf test (requires std::time::Instant measurement, not available in BAML):
//   - perf_large_int_array_uses_native_fast_path

// ── 1a: projection-valued associated-type binding in a blanket impl ─────────
//
// The throwaway scaffolding mirrors a fallible-ordering shape: `HasErr` carries
// an associated error `E`, and `Wrap` (playing `Sortable`) binds `WE` per-impl
// to the *projection* `(T as HasErr).E`.

interface HasErr {
    type E
    function f(self) -> int throws Self.E
}

interface Wrap {
    type WE
    function g(self) -> int throws Self.WE
}

implements<T extends HasErr> Wrap for T[] {
    type WE = T.E

    // NB: spelling matters throughout this impl. In throws position
    // neither `WE` nor `Self.WE` resolves (E0002), and the checker does
    // not unify the qualified projection `(T as HasErr).E` with the
    // body's inferred shorthand `T.E` (E0096/E0120) — so the binding,
    // the signature, and the body must all agree on the `T.E` spelling.
    function g(self) -> int throws T.E {
        if (self.length() == 0) { return 0 }
        return self[0].f()
    }
}

class Safe {
    value: int
    implements HasErr {
        type E = never
        function f(self) -> int throws never { return self.value }
    }
}

class Kaboom {
    message: string
}

class Risky {
    implements HasErr {
        type E = Kaboom
        function f(self) -> int throws Kaboom {
            throw Kaboom { message: "kaboom" }
        }
    }
}

// `Safe.E = never`, so `(Safe as HasErr).E = never` and `xs.g()` requires
// no error handling: the helper declares no `throws` and uses no `catch`.
function spike_1a_never_main() -> int throws never {
    let xs: Safe[] = [Safe { value: 7 }]
    return xs.g()
}

test "spike_1a_projection_normalizes_to_never_at_concrete_callsite" {
    assert.equal(spike_1a_never_main(), 7)
}

function spike_1a_catch_main() -> string {
    let xs: Risky[] = [Risky {}]
    return {
        let v = xs.g();
        "no throw"
    } catch (e) {
        Kaboom => "caught:" + e.message
    }
}

test "spike_1a_concrete_error_is_catchable_at_callsite" {
    assert.equal(spike_1a_catch_main(), "caught:kaboom")
}

// Inside generic code over `U extends HasErr`, `us.g()` stays symbolic:
// the wrapper declares `throws U.E` and re-throws. At the concrete call
// site the projection resolves to `Kaboom` and is caught.
function call_g<U extends HasErr>(us: U[]) -> int throws U.E {
    return us.g()
}

function spike_1a_symbolic_main() -> string {
    let xs: Risky[] = [Risky {}]
    return {
        let v = call_g(xs);
        "no throw"
    } catch (e) {
        Kaboom => "caught:" + e.message
    }
}

test "spike_1a_symbolic_projection_propagates_through_generic_code" {
    assert.equal(spike_1a_symbolic_main(), "caught:kaboom")
}

// ── 1b: blanket-impl method resolution on array receivers ───────────────────

interface Named {
    name: string
}

interface FirstNamed {
    function first_name(self) -> string throws never
}

implements<T extends Named> FirstNamed for T[] {
    function first_name(self) -> string {
        if (self.length() == 0) { return "" }
        return self[0].name
    }
}

class Person {
    name: string
    implements Named {}
}

// Direct call syntax on the array receiver — not via an interface-typed
// variable. Runtime result proves dispatch found the blanket impl.
function spike_1b_first_name_main() -> string {
    let xs: Person[] = [Person { name: "Ada" }, Person { name: "Bob" }]
    return xs.first_name()
}

test "spike_1b_blanket_method_resolves_on_array_receiver" {
    assert.equal(spike_1b_first_name_main(), "Ada")
}

// FINDING (deviates from the plan's expectation): BEP-044 documents
// class-method precedence on direct calls, but for structural array
// receivers the blanket impl's `count` (returning -1) currently wins over
// `Array.count()` (= length, 3). Pinned as-is.
interface Countable {
    function count(self) -> int throws never
}

implements<T> Countable for T[] {
    function count(self) -> int { return -1 }
}

function spike_1b_count_main() -> int {
    let xs: int[] = [1, 2, 3]
    return xs.count()
}

test "spike_1b_blanket_interface_method_wins_over_array_class_method" {
    assert.equal(spike_1b_count_main(), -1)
}

// ── 1c: two-`Self` method called through a bounded type variable ────────────

// Both arguments are the same type variable `T`, so the `Self`s provably
// match — exactly the shape of `a.cmp(b)` inside the blanket `sort`.
interface Cmp {
    function compare(self, other: Self) -> int throws never
}

class Num {
    value: int
    implements Cmp {
        function compare(self, other: Self) -> int {
            return self.value - other.value
        }
    }
}

function lt<T extends Cmp>(a: T, b: T) -> bool {
    return a.compare(b) < 0
}

function spike_1c_main() -> bool {
    return lt(Num { value: 1 }, Num { value: 2 })
}

test "spike_1c_two_self_method_through_bounded_typevar_runs" {
    assert.is_true(spike_1c_main())
}

// ── Phase 2 repro: user-class Compare dispatch ───────────────────────────────

class Score {
    points: int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.points == other.points }
    }
    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering throws never {
            if (self.points < other.points) { baml.ops.Ordering.Less }
            else if (self.points > other.points) { baml.ops.Ordering.Greater }
            else { baml.ops.Ordering.Equal }
        }
    }
}

function phase2_compare_main() -> baml.ops.Ordering throws never {
    let high = Score { points: 9 }
    let low = Score { points: 1 }
    return high.cmp(low)
}

function phase2_compare_explicit_binding_main() -> baml.ops.Ordering throws never {
    let high = Score { points: 9 }
    let low = Score { points: 1 }
    let result = high.cmp(low)
    return result
}

test "phase2_user_class_compare_direct_call_in_main" {
    assert.is_true(phase2_compare_main() == baml.ops.Ordering.Greater)
}

test "phase2_user_class_compare_with_explicit_binding_in_main" {
    assert.is_true(phase2_compare_explicit_binding_main() == baml.ops.Ordering.Greater)
}

// ── Phase 3 runtime: actually execute the blanket `sort` ───────────────────

function phase3_sort_ints_main() -> int throws never {
    let xs = [3, 1, 2]
    let s = xs.sort()
    return s[0] * 100 + s[1] * 10 + s[2]
}

test "phase3_runtime_sort_ints" {
    assert.equal(phase3_sort_ints_main(), 123)
}

// ── Phase 3 runtime gap-mapping: generic dispatch to primitive vs class ─────

function three_way<T extends baml.ops.Compare>(a: T, b: T) -> baml.ops.Ordering throws never {
    return a.cmp(b)
}

function phase3_generic_compare_primitive_main() -> baml.ops.Ordering throws never {
    let a: int = 3
    let b: int = 1
    return three_way(a, b)
}

test "phase3_runtime_generic_compare_on_primitive" {
    assert.is_true(phase3_generic_compare_primitive_main() == baml.ops.Ordering.Greater)
}

// Named `ScorePts` (not `Score`) to avoid collision with phase2's `Score` above.
// Shared definition for the three phase3 user-class tests.
class ScorePts {
    points: int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.points == other.points }
    }
    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering throws never {
            return self.points.cmp(other.points)
        }
    }
}

function phase3_generic_compare_user_class_main() -> baml.ops.Ordering throws never {
    return three_way(ScorePts { points: 9 }, ScorePts { points: 1 })
}

test "phase3_runtime_generic_compare_on_user_class" {
    assert.is_true(phase3_generic_compare_user_class_main() == baml.ops.Ordering.Greater)
}

function phase3_sort_user_class_main() -> int throws never {
    let xs = [ScorePts { points: 3 }, ScorePts { points: 1 }, ScorePts { points: 2 }]
    let sorted = xs.sort()
    return sorted[0].points
}

test "phase3_runtime_sort_user_class" {
    assert.equal(phase3_sort_user_class_main(), 1)
}

// Same body as the blanket `sort`, but as a plain generic function — isolates
// whether a failure is the blanket-impl context or generic lambda dispatch.
function gsort<T extends baml.ops.Compare>(xs: T[]) -> T[] throws never {
    return xs.sort_by((a: T, b: T) -> baml.ops.Ordering throws never { a.cmp(b) })
}

function phase3_gsort_ints_main() -> int throws never {
    let xs = [3, 1, 2]
    let s = gsort(xs)
    return s[0] * 100 + s[1] * 10 + s[2]
}

test "phase3_runtime_generic_fn_sort_by_compare_ints" {
    assert.equal(phase3_gsort_ints_main(), 123)
}

// User-code generic `sort_by`-with-`cmp`-lambda over a *user class*. Same shape
// as the stdlib `Sortable.sort` fallback: `a.cmp(b)` on a bounded type variable
// lowers to a virtual call resolved on the runtime type.
function phase3_gsort_user_class_main() -> int throws never {
    let xs = [ScorePts { points: 3 }, ScorePts { points: 1 }, ScorePts { points: 2 }]
    let s = gsort(xs)
    return s[0].points * 100 + s[1].points * 10 + s[2].points
}

test "phase3_runtime_user_generic_sort_by_compare_user_class" {
    assert.equal(phase3_gsort_user_class_main(), 123)
}

// Param-receiver dispatch in a real function (not a test block).
function do_sort(xs: int[]) -> int[] throws never {
    return xs.sort()
}

function phase3_param_receiver_main() -> int throws never {
    let xs = [3, 1, 2]
    let s = do_sort(xs)
    return s[0] * 100 + s[1] * 10 + s[2]
}

test "phase3_runtime_sort_param_receiver_in_function" {
    assert.equal(phase3_param_receiver_main(), 123)
}

// ── Phase 5: a class that DOES implement Compare sorts fine ─────────────────

class Resume {
    rank: int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.rank == other.rank }
    }
    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering throws never {
            return self.rank.cmp(other.rank)
        }
    }
}

function phase5_resume_sort_main() -> int throws never {
    let xs = [Resume { rank: 3 }, Resume { rank: 1 }, Resume { rank: 2 }]
    let sorted = xs.sort()
    return sorted[0].rank
}

test "phase5_class_with_comparable_sort_compiles_and_runs" {
    assert.equal(phase5_resume_sort_main(), 1)
}

// ── 02 Phase 1: dispatch shape for the primitive fast path ──────────────────

// A BAML-level `is int` on a `T`-typed value tests against the *realized* `T`
// from the enclosing frame (overlap-oracle narrowing): an actual int reports
// true, a string false. It used to lower to a constant-false test (`T ∩ int`
// intersected to `never`), which is why `Sortable.sort`'s primitive fast-path
// test went native; with this working, that dispatch could move back to pure
// BAML.
function probe<T>(x: T) -> int {
    if (x is int) { 1 } else { 0 }
}

function is_primitive_probe_main() -> int {
    let a: int = 42
    return probe(a) * 10 + probe("s")
}

test "is_primitive_on_generic_value_tests_realized_type" {
    assert.equal(is_primitive_probe_main(), 10)
}

// The fallback dispatch shape: a boolean guard routing between two generic
// callees instantiated at the *symbolic* `T` (return `T[]`, throws
// `T`-bounded) with no refinement anywhere. `is_fast` stands in for the
// native `_is_primitive_array`; `fast_g` for `_rust_sort`; `slow_g` for the
// `sort_by` path.
function is_fast<T extends baml.ops.Compare>(xs: T[]) -> bool throws never {
    return xs.length() == 0
}

function fast_g<T extends baml.ops.Compare>(xs: T[]) -> T[] throws never {
    return xs
}

function slow_g<T extends baml.ops.Compare>(xs: T[]) -> T[] throws never {
    return xs
}

function dispatch_f<T extends baml.ops.Compare>(xs: T[]) -> T[] throws never {
    if (is_fast(xs)) {
        fast_g(xs)
    } else {
        slow_g(xs)
    }
}

class RouteItem {
    rank int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.rank == other.rank }
    }
    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering throws never {
            self.rank.cmp(other.rank)
        }
    }
}

// The dispatch shape executes correctly at runtime for a primitive and a
// user-class element type, through both branches (empty → `is_fast` true
// in the stand-in, non-empty → the slow branch). Real routing through the
// native `_is_primitive_array` is pinned by the Phase 4/5 sort tests.
function element_dispatch_main() -> int throws never {
    let empty: int[] = []
    let ints = [3, 1]
    let items = [RouteItem { rank: 7 }]
    if (dispatch_f(empty).length() != 0) { return -1 }
    if (dispatch_f(ints).length() != 2) { return -2 }
    return dispatch_f(items)[0].rank
}

test "element_is_dispatch_runtime_both_branches" {
    assert.equal(element_dispatch_main(), 7)
}

// ── 02 Phase 5: parity for the native fast path ──────────────────────────────

// The native natural sort and the `sort_by` + `compare` path must produce
// identical orderings for every primitive domain — including float with
// NaN/±inf/±0, where both paths must agree on BAML's total float order. The
// user-class domain rides along as a sanity check (both sides take the
// comparator path there).
class ParityItem {
    rank int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.rank == other.rank }
    }
    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering throws never {
            self.rank.cmp(other.rank)
        }
    }
}

// Element-wise `compare(...) == 0` rather than `xs == ys`, so the check is on
// the very ordering the two sort paths are being compared on rather than on
// `==`. `Compare for float` is BAML's total float order, which is total over
// the NaN/±inf/±0 domain below.
function check<T extends baml.ops.Compare>(xs: T[], ys: T[]) -> bool throws never {
    xs.sort();
    ys.sort_by((a: T, b: T) -> baml.ops.Ordering throws never { a.cmp(b) });
    if (xs.length() != ys.length()) { return false }
    let i = 0;
    while (i < xs.length()) {
        if (xs[i].cmp(ys[i]) != baml.ops.Ordering.Equal) { return false }
        i = i + 1;
    }
    return true
}

function parity_main() -> int throws never {
    if (!check([3, 1, 2, 1, 3], [3, 1, 2, 1, 3])) { return 1 }
    let f = [2.5, float.nan(), 0.0 - float.inf(), 0.5, float.inf(), 0.0, -0.0, float.nan(), 1.5]
    let g = [2.5, float.nan(), 0.0 - float.inf(), 0.5, float.inf(), 0.0, -0.0, float.nan(), 1.5]
    if (!check(f, g)) { return 2 }
    if (!check(["b", "a", "aa", "b"], ["b", "a", "aa", "b"])) { return 3 }
    if (!check([3n, 1n, 2n, 1n], [3n, 1n, 2n, 1n])) { return 4 }
    if (!check(
        [ParityItem { rank: 3 }, ParityItem { rank: 1 }, ParityItem { rank: 2 }],
        [ParityItem { rank: 3 }, ParityItem { rank: 1 }, ParityItem { rank: 2 }]
    )) { return 5 }
    return 0
}

test "parity_fast_path_matches_comparator_path" {
    assert.equal(parity_main(), 0)
}

// Tests float NaN sorting through the native fast path (infallible, NaN after +inf).
function float_nan_sort_main() -> bool throws never {
    let xs = [1.0, float.nan(), 0.5, float.inf()]
    xs.sort()
    if (xs.at(0) != 0.5 || xs.at(1) != 1.0 || xs.at(2) != float.inf()) { return false }
    match (xs.at(3)) {
        null => false
        let f: float => f.is_nan()
    }
}

test "float_nan_sorts_via_total_order_no_throw" {
    assert.is_true(float_nan_sort_main())
}

// An *out-of-body* `implements baml.ops.Compare for <class>` registers
// `cmp` under the synthetic `Compare$for$<class>` class (as spelled at
// the impl site), not under the class. `make_compare_callee` must find it
// there — the in-body-only lookup made the sort type-check but fail at
// runtime with "compare dispatch: function … not found".
class OobScore {
    points: int
}

implements baml.ops.Equals for OobScore {
    function eq(self, other: Self) -> bool throws never { self.points == other.points }
}

implements baml.ops.Compare for OobScore {
    function cmp(self, other: Self) -> baml.ops.Ordering throws never {
        self.points.cmp(other.points)
    }
}

function oob_score_sort_main() -> int throws never {
    let xs = [OobScore { points: 3 }, OobScore { points: 1 }, OobScore { points: 2 }]
    let s = xs.sort()
    return s[0].points * 100 + s[1].points * 10 + s[2].points
}

test "sort_user_class_with_out_of_body_comparable_impl" {
    assert.equal(oob_score_sort_main(), 123)
}

// Phase 2/4 interaction regression: a user `Compare` class routes through
// native `sort_by` + the `a.cmp(b)` lambda (the dispatch's fallback
// branch) and still sorts stably, in place, returning self.
class Ranked {
    rank int
    tag string
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.rank == other.rank }
    }
    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering throws never {
            self.rank.cmp(other.rank)
        }
    }
}

function ranked_sort_main() -> string throws never {
    let xs = [
        Ranked { rank: 2, tag: "a" },
        Ranked { rank: 1, tag: "b" },
        Ranked { rank: 2, tag: "c" },
        Ranked { rank: 1, tag: "d" }
    ]
    let result = xs.sort()
    if (result != xs) { return "not self" }
    return xs.map((x: Ranked) -> string { x.tag }).join("")
}

test "user_class_still_sorts_through_comparator_path" {
    assert.equal(ranked_sort_main(), "bdac")
}
