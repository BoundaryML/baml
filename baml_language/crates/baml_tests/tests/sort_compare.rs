//! Spike tests for the `Sortable` array-sort design, now built on
//! `baml.ops.Compare`
//! (thoughts/sam-projects/array-sort/01b-option-5-tdd-plan.md, Phase 1).
//!
//! These tests pin the three pieces of type-system machinery the design
//! depends on, using throwaway interfaces so failures point at the compiler
//! rather than at the stdlib:
//!
//!   1a. An associated-type *binding whose value is a projection off the
//!       impl's type variable* (`type WE = (T as HasErr).E`) inside a blanket
//!       impl for `T[]`, with the projection used in `throws` position and
//!       normalized at concrete call sites (`never` ⇒ no handling required).
//!   1b. Method resolution of blanket-impl interface methods on *array
//!       receivers* via direct call syntax (`xs.method()`), the call-site
//!       error for non-qualifying element types, and the BEP-044 rule that a
//!       class method shadows a same-named interface method.
//!   1c. A two-`Self` method (`compare(self, other: Self)`) called through a
//!       bounded type variable.
//!
//! The stdlib `Sortable` work builds on these; see
//! `baml_src/ns_arrays/arrays.baml` for the runtime characterization net.
//! `baml.ops.Compare` carries no associated error (`cmp` is `throws never`),
//! so the associated-type machinery is pinned only through the throwaway
//! interfaces here.

use std::collections::HashSet;

use baml_compiler_diagnostics::Severity;
use baml_tests::{
    baml_test,
    stdlib_prefix::{check_user_files, setup_test_db},
};
use bex_engine::BexExternalValue;

fn collect_compile_errors(source: &str) -> Vec<String> {
    let db = setup_test_db(source);
    let all_files = db.workspace_files();
    let user_file_ids: HashSet<_> = all_files.iter().map(|f| f.file_id(&db)).collect();

    check_user_files(&db)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .filter(|d| {
            d.primary_span()
                .map(|span| user_file_ids.contains(&span.file_id))
                .unwrap_or(false)
        })
        .map(|d| format!("[{}] {}", d.code(), d.message_with_primary_label()))
        .collect()
}

#[track_caller]
fn assert_zero_compile_errors(source: &str) {
    let errors = collect_compile_errors(source);
    assert!(
        errors.is_empty(),
        "expected zero compile errors, got:\n  {}",
        errors.join("\n  ")
    );
}

#[track_caller]
fn assert_compile_error_contains(source: &str, needle: &str) {
    let errors = collect_compile_errors(source);
    assert!(
        errors.iter().any(|e| e.contains(needle)),
        "expected a compile error containing {needle:?}; got:\n  {}",
        errors.join("\n  ")
    );
}

// ── 1a: projection-valued associated-type binding in a blanket impl ─────────
//
// The throwaway scaffolding mirrors a fallible-ordering shape: `HasErr`
// carries an associated error `E`, and `Wrap` (playing `Sortable`) binds `WE`
// per-impl to the *projection* `(T as HasErr).E`.

const SPIKE_1A_SCAFFOLD: &str = r#"
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
"#;

#[test]
fn spike_1a_projection_valued_assoc_binding_in_blanket_impl_compiles() {
    assert_zero_compile_errors(SPIKE_1A_SCAFFOLD);
}

#[test]
fn spike_1a_projection_with_concrete_error_requires_handling() {
    // `Risky.E = Kaboom`, so `xs.g()` throws `Kaboom` at the call site; a
    // caller that neither catches nor declares it must not compile.
    assert_compile_error_contains(
        &format!(
            r#"
            {SPIKE_1A_SCAFFOLD}

            class Kaboom {{
                message: string
            }}

            class Risky {{
                implements HasErr {{
                    type E = Kaboom
                    function f(self) -> int throws Kaboom {{
                        throw Kaboom {{ message: "kaboom" }}
                    }}
                }}
            }}

            // An undeclared `throws` is *inferred*, so the unhandled-error
            // check only fires against an explicit declaration.
            function main() -> int throws never {{
                let xs: Risky[] = [Risky {{}}]
                return xs.g()
            }}
            "#
        ),
        "Kaboom",
    );
}

#[test]
fn spike_1a_symbolic_projection_in_generic_signature_compiles() {
    assert_zero_compile_errors(&format!(
        r#"
        {SPIKE_1A_SCAFFOLD}

        function call_g<U extends HasErr>(us: U[]) -> int throws U.E {{
            return us.g()
        }}
        "#
    ));
}

#[test]
fn spike_1a_diagnostics_path_survives_non_qualifying_projection() {
    // BEP-057 flags TIR projection-on-type-variable normalization as the
    // historically crash-prone layer. Feed the diagnostics pipeline a call
    // site whose element type does NOT satisfy the bound: we must get
    // ordinary diagnostics back (any number), never a panic.
    let errors = collect_compile_errors(&format!(
        r#"
        {SPIKE_1A_SCAFFOLD}

        class NoImpl {{
            value: int
        }}

        function main() -> int {{
            let xs: NoImpl[] = [NoImpl {{ value: 1 }}]
            return xs.g()
        }}
        "#
    ));
    assert!(
        !errors.is_empty(),
        "expected a call-site error for a non-qualifying element type"
    );
}

// ── 1b: blanket-impl method resolution on array receivers ───────────────────

const SPIKE_1B_SCAFFOLD: &str = r#"
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
"#;

#[test]
fn spike_1b_non_qualifying_element_is_compile_error_at_callsite() {
    // `Rock` does not implement `Named`, so `rocks.first_name()` must fail at
    // the call site. This is the exact error a user will see for
    // `Resume[].sort()` — Phase 6 polishes its wording; here we only pin that
    // it exists and names the offending type.
    assert_compile_error_contains(
        &format!(
            r#"
            {SPIKE_1B_SCAFFOLD}

            class Rock {{
                label: string
            }}

            function main() -> string {{
                let rocks: Rock[] = [Rock {{ label: "igneous" }}]
                return rocks.first_name()
            }}
            "#
        ),
        "Rock",
    );
}

// ── 1c: two-`Self` method called through a bounded type variable ────────────

#[test]
fn spike_1c_interface_typed_values_cannot_call_two_self_method() {
    // Guard against accidental loosening of the existing `Self` rule: two
    // *interface-typed* values have unprovable `Self`s and must not compile.
    assert_compile_error_contains(
        r#"
        interface Cmp {
            function compare(self, other: Self) -> int throws never
        }

        function bad(a: Cmp, b: Cmp) -> bool {
            return a.compare(b) < 0
        }
        "#,
        "Self",
    );
}

// ── Phase 2 repro: user-class Compare dispatch (fast harness) ──────────────

#[test]
fn missing_undefaulted_assoc_binding_is_error() {
    // An UNDEFAULTED associated type must be bound by every impl — a compile
    // error (E0001) distinct from the method's `throws` (E0120). The stdlib no
    // longer has such an interface (`baml.ops.Compare` carries no associated
    // type since `cmp` is `throws never`), so this uses a throwaway one.
    assert_compile_error_contains(
        r#"
        interface Ordered {
            type CompareError
            function compare(self, other: Self) -> int throws Self.CompareError
        }
        class Score {
            points: int
            implements Ordered {
                function compare(self, other: Self) -> int throws never {
                    if (self.points < other.points) { -1 }
                    else if (self.points > other.points) { 1 }
                    else { 0 }
                }
            }
        }
        "#,
        "CompareError",
    );
}

#[tokio::test]
async fn user_class_cmp_direct_call_mir_optimized() {
    let output = baml_tests::baml_test_optimized!(
        r#"
        class Score {
            points: int
            implements baml.ops.Equals {
                function eq(self, other: Self) -> bool throws never { self.points == other.points }
            }
            implements baml.ops.Compare {
                function cmp(self, other: Self) -> baml.ops.Ordering throws never {
                    self.points.cmp(other.points)
                }
            }
        }

        function main() -> int throws never {
            let high = Score { points: 9 }
            let low = Score { points: 1 }
            return match (high.cmp(low)) {
                baml.ops.Ordering.Less => -1,
                baml.ops.Ordering.Equal => 0,
                baml.ops.Ordering.Greater => 1,
            }
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(1));
}

// ── Phase 3: `Sortable` blanket impl + projection normalization ─────────────
//
// Canonical scaffold mirrors a fallible-ordering shape: a `compare`-style interface
// with an UNDEFAULTED associated error (`type CE`, no `= never`), a
// `Sortable`-style blanket impl binding `SE = T.CE`, and user classes that
// bind `CE` explicitly (to `never` or a concrete error).
//
// Defaulting the associated error would work too — a bare bound pins nothing,
// so an implementor that overrides the default still satisfies it (see
// `phase3_defaulted_assoc_override_satisfies_bare_bound` below). The stdlib
// leaves its associated error undefaulted as a matter of style, not
// necessity.

const PHASE3_SCAFFOLD: &str = r#"
    interface Cmp2 {
        type CE
        function comp(self, other: Self) -> baml.ops.Ordering throws Self.CE
    }

    interface Srt2 {
        type SE
        function srt(self) -> Self throws Self.SE
    }

    implements<T extends Cmp2> Srt2 for T[] {
        type SE = T.CE

        function srt(self) -> Self throws T.CE {
            self.sort_by((a: T, b: T) -> baml.ops.Ordering throws T.CE { a.comp(b) })
        }
    }

    class Boom2 { message: string }

    class WithNever {
        v: int
        implements Cmp2 {
            type CE = never
            function comp(self, other: Self) -> baml.ops.Ordering throws never { return baml.ops.Ordering.Equal }
        }
    }

    class WithErr {
        v: int
        implements Cmp2 {
            type CE = Boom2
            function comp(self, other: Self) -> baml.ops.Ordering throws Boom2 { return baml.ops.Ordering.Equal }
        }
    }
"#;

#[test]
fn phase3_never_binding_callsite_normalizes_to_never() {
    // `WithNever.CE = never`, so `(WithNever[] as Srt2).SE` normalizes to
    // `never`: `srt()` requires no error handling.
    assert_zero_compile_errors(&format!(
        r#"
        {PHASE3_SCAFFOLD}
        function f(xs: WithNever[]) -> WithNever[] throws never {{
            xs.srt()
        }}
        "#
    ));
}

#[test]
fn phase3_error_binding_callsite_throws_it() {
    // `WithErr.CE = Boom2`, so `srt()` throws `Boom2` and the caller must
    // declare it (here) or catch it.
    assert_zero_compile_errors(&format!(
        r#"
        {PHASE3_SCAFFOLD}
        function f(xs: WithErr[]) -> WithErr[] throws Boom2 {{
            xs.srt()
        }}
        "#
    ));
}

#[test]
fn phase3_error_binding_unhandled_is_compile_error() {
    // The dual of the previous test: failing to handle `Boom2` is an error.
    assert_compile_error_contains(
        &format!(
            r#"
            {PHASE3_SCAFFOLD}
            function f(xs: WithErr[]) -> WithErr[] throws never {{
                xs.srt()
            }}
            "#
        ),
        "Boom2",
    );
}

// Issue-A regression: a projection off a *primitive* base type
// (`int.CE` via `implements Cmp2 for int`) must normalize. Before the
// `resolve_primitive_projection` fix this stayed symbolic and `int[].srt()`
// spuriously "threw `int.CE`".
#[test]
fn phase3_out_of_body_impl_on_builtin_normalizes_to_never() {
    assert_zero_compile_errors(&format!(
        r#"
        {PHASE3_SCAFFOLD}

        implements Cmp2 for int {{
            type CE = never
            function comp(self, other: Self) -> baml.ops.Ordering throws never {{ return baml.ops.Ordering.Equal }}
        }}

        function f(xs: int[]) -> int[] throws never {{
            xs.srt()
        }}
        "#
    ));
}

#[test]
fn phase3_out_of_body_impl_on_builtin_with_error_throws_it() {
    assert_zero_compile_errors(&format!(
        r#"
        {PHASE3_SCAFFOLD}

        implements Cmp2 for int {{
            type CE = Boom2
            function comp(self, other: Self) -> baml.ops.Ordering throws Boom2 {{ return baml.ops.Ordering.Equal }}
        }}

        function f(xs: int[]) -> int[] throws Boom2 {{
            xs.srt()
        }}
        "#
    ));
}

// A defaulted associated type does not constrain a bare bound: `T extends
// CmpD` pins nothing, so an implementor that overrides the default (a
// fallible comparator with `CE = BoomD`) still satisfies the blanket impl and
// `srt` resolves — the override's error type flows through `SE = T.CE`.
#[test]
fn phase3_defaulted_assoc_override_satisfies_bare_bound() {
    let errors = collect_compile_errors(
        r#"
        interface CmpD {
            type CE = never
            function comp(self, other: Self) -> baml.ops.Ordering throws Self.CE
        }

        interface SrtD {
            type SE
            function srt(self) -> Self throws Self.SE
        }

        implements<T extends CmpD> SrtD for T[] {
            type SE = T.CE
            function srt(self) -> Self throws T.CE {
                self.sort_by((a: T, b: T) -> baml.ops.Ordering throws T.CE { a.comp(b) })
            }
        }

        class BoomD { message: string }

        class WithErrD {
            v: int
            implements CmpD {
                type CE = BoomD
                function comp(self, other: Self) -> baml.ops.Ordering throws BoomD { return baml.ops.Ordering.Equal }
            }
        }

        function f(xs: WithErrD[]) -> WithErrD[] throws BoomD {
            xs.srt()
        }
        "#,
    );
    assert!(
        errors.is_empty(),
        "a defaulted-then-overridden associated type must satisfy the bare bound; got:\n  {}",
        errors.join("\n  ")
    );
}

// ── Phase 5: deliberate breaking changes (now compile errors) ───────────────
//
// After the cutover, `sort()` is the `Sortable` blanket method requiring
// `T implements baml.ops.Compare`. Element types that cannot implement it
// — unions (including optionals) and user classes without an impl — no longer
// compile, where the old native `Array.sort` accepted them and threw at
// runtime.

#[test]
fn phase5_union_int_float_sort_is_compile_error() {
    // A union element can't implement `baml.ops.Compare`, so the `Sortable`
    // blanket impl does not apply and the error names the union.
    assert_compile_error_contains(
        r#"
        function f() -> null throws never {
            let xs: (int | float)[] = [1, 2.5]
            xs.sort()
            return null
        }
        "#,
        "int | float",
    );
}

#[test]
fn phase5_optional_element_sort_is_compile_error() {
    // Optional = union with null; `sort` doesn't resolve on the array at all
    // (E0007 names the offending array type and the missing member).
    let source = r#"
        function f() -> null throws never {
            let xs: int?[] = [1, null, 2]
            xs.sort()
            return null
        }
        "#;
    assert_compile_error_contains(source, "(int | null)[]");
    assert_compile_error_contains(source, "no member `sort`");
}

#[test]
fn phase5_sort_by_key_nullable_key_is_compile_error() {
    // `sort_by_key` requires `U extends baml.ops.Compare` (it orders by the
    // key's natural order). A nullable key (`int?` = a union) cannot implement
    // `Compare`, so the bound is unsatisfied (named after the union key type) —
    // replacing the old runtime `InvalidArgument` rejection.
    assert_compile_error_contains(
        r#"
        function f() -> null throws never {
            let xs = [3, 1, 2]
            xs.sort_by_key((x: int) -> int? { if (x == 1) { null } else { x } })
            return null
        }
        "#,
        "int | null",
    );
}

#[test]
fn phase5_union_int_string_sort_is_compile_error() {
    assert_compile_error_contains(
        r#"
        function f() -> null throws never {
            let xs: (int | string)[] = [2, "x", 1]
            xs.sort()
            return null
        }
        "#,
        "int | string",
    );
}

#[test]
fn phase5_class_without_compare_sort_is_compile_error() {
    // E0007: the array type has no `sort` member (no `Compare` impl, so
    // the `Sortable` blanket impl doesn't apply). phase6 pins the full
    // message shape; here we pin the offending type and member.
    let source = r#"
        class Resume {
            name: string
        }
        function f() -> null throws never {
            let xs = [Resume { name: "b" }, Resume { name: "a" }]
            xs.sort()
            return null
        }
        "#;
    assert_compile_error_contains(source, "Resume[]");
    assert_compile_error_contains(source, "sort");
}

// ── 02 Phase 1: dispatch shape for the primitive fast path ──────────────────
//
// Spikes for thoughts/sam-projects/array-sort/02-native-sort-fast-path-plan.md.
//
// HISTORICAL CONTEXT, updated: when the native dispatch was designed, the
// plan's primary `match (self) { int[] => …, bigint[] => …, …, _ => … }`
// shape failed for *matching* reasons — array patterns lowered to the coarse
// LIST tag (no element discrimination), the first array arm statically
// covered all of `T[]` making later arms `unreachable arm` errors, and a
// BAML-level `x is int` on a `T`-typed value folded to a constant-false test.
// Those limitations are fixed: array type arms emit invariant
// element-discriminating tests, a concrete array arm over `T[]` is
// possible-but-not-covering (all arms reachable, `_` required), and `is int`
// tests the realized frame `T` (see
// `is_primitive_on_generic_value_tests_realized_type` in the corpus:
// baml_src/ns_comparable_sort/comparable_sort.baml).
//
// The shape still cannot be *typed*, though, for a different reason: inside
// the `int[]` arm the scrutinee narrows to `int[]`, so `fast_g(xs)` infers
// `T = int` and returns `int[]` — and the checker has no type-variable
// refinement ("`T = int` within this arm" is per-realization knowledge it
// does not track), so `int[]` cannot flow back to the enclosing `T[]` return.
// Hence the shipped dispatch keeps the single native boolean
// `root._is_primitive_array(self)` (reads the first element's runtime tag,
// routing to one `root._rust_sort(self)`) — `T` stays fully symbolic and the
// native test dispatches on runtime *values*, immune to type-arg plumbing.
// `is_fast` stands in for the native boolean; `fast_g` for `_rust_sort`;
// `slow_g` for the `sort_by` path.

#[test]
fn match_dispatch_array_type_arms_reachable_but_t_is_not_refined() {
    // The arms are all reachable now (no `unreachable arm`), but each arm's
    // body fails the return check: the narrowed `int[]` result cannot be
    // returned as `T[]` without type-variable refinement.
    let errors = collect_compile_errors(
        r#"
        function fast_g<T extends baml.ops.Compare>(xs: T[]) -> T[] throws never {
            return xs
        }

        function dispatch_f<T extends baml.ops.Compare>(xs: T[]) -> T[] throws never {
            match (xs) {
                int[] => fast_g(xs),
                bigint[] => fast_g(xs),
                string[] => fast_g(xs),
                float[] => fast_g(xs),
                _ => xs,
            }
        }
        "#,
    );
    assert!(
        !errors.iter().any(|e| e.contains("unreachable arm")),
        "concrete array arms over T[] are reachable, got:\n  {}",
        errors.join("\n  ")
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("expected `T[]`, found `int[]`")),
        "expected the un-refined return mismatch, got:\n  {}",
        errors.join("\n  ")
    );
}

const ELEMENT_DISPATCH_SCAFFOLD: &str = r#"
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
"#;

#[test]
fn element_is_dispatch_compiles() {
    // The fallback shape: a boolean guard routing between two generic callees
    // instantiated at the *symbolic* `T` (return `T[]`) with no refinement
    // anywhere.
    assert_zero_compile_errors(ELEMENT_DISPATCH_SCAFFOLD);
}

#[test]
fn element_is_dispatch_int_callsite_normalizes_to_never() {
    assert_zero_compile_errors(&format!(
        r#"
        {ELEMENT_DISPATCH_SCAFFOLD}
        function use_int(xs: int[]) -> int[] throws never {{
            dispatch_f(xs)
        }}
        "#
    ));
}

#[test]
fn element_is_dispatch_float_callsite_normalizes_to_never() {
    // Relies on the float decision: `Compare for float` is BAML's total float
    // order and `throws never`, so a `float[]` call site needs no handling.
    assert_zero_compile_errors(&format!(
        r#"
        {ELEMENT_DISPATCH_SCAFFOLD}
        function use_float(xs: float[]) -> float[] throws never {{
            dispatch_f(xs)
        }}
        "#
    ));
}

// ── 02 Phase 5: performance / parity for the native fast path ───────────────

#[tokio::test]
async fn perf_large_int_array_uses_native_fast_path() {
    // 10k pseudo-random ints. On the native fast path this is instant; the
    // comparator path (CPS insertion sort) would make O(n²) ≈ 10⁷–10⁸ yields
    // into BAML and blow the coarse bound below by orders of magnitude — the
    // bound is the assertion that primitives no longer route through
    // `sort_by` + an `a.cmp(b)` comparator.
    let start = std::time::Instant::now();
    let output = baml_test!(
        r#"
        function main() -> int throws never {
            let xs: int[] = []
            let seed = 42
            let i = 0
            while (i < 10000) {
                seed = (seed * 1103515245 + 12345) % 2147483648
                xs.push(seed)
                i += 1
            }
            let result = xs.sort()
            if (result != xs) { return -3 }
            let j = 1
            while (j < 10000) {
                match (xs.at(j - 1)) {
                    null => { return -1 }
                    let a: int => {
                        match (xs.at(j)) {
                            null => { return -1 }
                            let b: int => { if (a > b) { return -2 } }
                        }
                    }
                }
                j += 1
            }
            return xs.length()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(10000));
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(60),
        "10k-int sort took {elapsed:?}; the native fast path should be far \
         under this bound — did primitives fall back to the comparator path?"
    );
}

#[test]
fn phase6_sort_error_message_names_the_array_type() {
    // The `Resume[].sort()` failure (no `Compare` impl) names the array type
    // and the missing `sort` member, with a stable `E0007` code. (A richer
    // "implement baml.ops.Compare or use sort_by" hint would live in the diagnostic
    // rendering layer rather than the `TirTypeError` Display — deferred.)
    let errors = collect_compile_errors(
        r#"
        class Resume { name: string }
        function f() -> null throws never {
            let xs = [Resume { name: "b" }, Resume { name: "a" }]
            xs.sort()
            return null
        }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.starts_with("[E0007]") && e.contains("Resume[]") && e.contains("sort")),
        "expected a typed `no member sort` error on `Resume[]`; got:\n  {}",
        errors.join("\n  ")
    );
}
