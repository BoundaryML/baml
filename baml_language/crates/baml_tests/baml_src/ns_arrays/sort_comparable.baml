// Tests for the `Comparable` interface (stdlib `comparable.baml`) and the
// `Sortable`-based array sort built on top of it.
//
// Phase 2 of thoughts/sam-projects/array-sort/01b-option-5-tdd-plan.md
// (float semantics updated by the 02 follow-up): builtin impls for
// int/float/bigint/string, `type CompareError = never` for all four — float
// orders by IEEE 754 totalOrder (`f64::total_cmp`), so NaN has a defined
// position and `compare` never throws.

// NB on shape: receivers are *annotated* test-block locals and arguments are
// literals (or inline calls). The annotation widens the receiver's literal
// type — `(5).compare(3)` would bind `Self` to the literal type `5` and
// reject `3` — and literal arguments widen into `Self` fine. Arguments must
// not be test-block locals: a local passed as the `other` argument of a
// two-`Self` interface method arrives boxed in a `test` block (pre-existing
// VM quirk, see the user-class section note below) and the comparison op
// rejects the box. Receiver-position locals unbox fine.

test "comparable_int_compare" {
    let five: int = 5;
    let three: int = 3;
    assert.is_true(five.compare(3) > 0);
    assert.is_true(three.compare(5) < 0);
    assert.is_true(three.compare(3) == 0)
}

test "comparable_string_compare" {
    // Annotated receivers widen the literal types ("a".compare("b") would
    // bind `Self` to the literal type "a" and reject "b"); literal arguments
    // widen into `Self` fine.
    let a: string = "a";
    let b: string = "b";
    let aa: string = "aa";
    assert.is_true(a.compare("b") < 0);
    assert.is_true(b.compare("a") > 0);
    assert.is_true(aa.compare("a") > 0);
    assert.is_true(a.compare("a") == 0)
}

test "comparable_bigint_compare" {
    let one: bigint = 1n;
    let two: bigint = 2n;
    assert.is_true(two.compare(1n) > 0);
    assert.is_true(one.compare(2n) < 0);
    assert.is_true(two.compare(2n) == 0)
}

test "comparable_float_compare" {
    // Same literal-type widening dance as the string test above.
    let lo: float = 1.5;
    let hi: float = 2.5;
    assert.is_true(hi.compare(1.5) > 0);
    assert.is_true(lo.compare(2.5) < 0);
    assert.is_true(lo.compare(1.5) == 0)
}

// DECISION TEST (02-plan Phase 3, reverses the 01b NaN decision): float
// comparison no longer throws on NaN — `total_cmp` orders NaN after +inf
// (`… < +inf < +NaN`) and equal to itself.
test "comparable_float_nan_total_order" {
    let one: float = 1.0;
    assert.is_true(one.compare(float.nan()) < 0);
    assert.is_true(float.nan().compare(1.0) > 0);
    assert.is_true(float.nan().compare(float.inf()) > 0);
    assert.is_true(float.nan().compare(float.nan()) == 0)
}

// ─── user classes opting in ──────────────────────────────────────────────────
//
// KNOWN VM BUG (pre-existing, not Comparable-specific): a class instance read
// back from a *test-block local* stays boxed, and passing a boxed value to a
// two-`Self`-param interface method (`compare(self, other: Self)`) panics the
// VM with "unreachable code executed". The same calls work from ordinary
// functions. Workaround used throughout: construct instances at the call
// boundary (inline arguments), never via test-block `let`.

// Total-order user type: binds `type CompareError = never` and writes the
// matching `throws never` (an omitted `throws` is inferred, not `never`, and
// fails conformance E0120).
class ComparableScore {
    points int
    implements baml.Comparable {
        type CompareError = never
        function compare(self, other: Self) -> int throws never {
            self.points.compare(other.points)
        }
    }
}

test "comparable_user_class_default_cmperror_is_never" {
    assert.is_true(ComparableScore { points: 9 }.compare(ComparableScore { points: 1 }) > 0);
    assert.is_true(ComparableScore { points: 1 }.compare(ComparableScore { points: 9 }) < 0);
    assert.is_true(ComparableScore { points: 1 }.compare(ComparableScore { points: 1 }) == 0)
}

// Fallible comparator: binds `type CompareError = ComparableCompareError` and throws.
class ComparableCompareError {
    message string
}

class ComparableFlaky {
    value int?
    implements baml.Comparable {
        type CompareError = ComparableCompareError

        function compare(self, other: Self) -> int throws ComparableCompareError {
            if let a: int = self.value {
                if let b: int = other.value {
                    return a.compare(b)
                }
            }
            throw ComparableCompareError { message: "cannot compare null" }
        }
    }
}

// The fallible comparator's happy path: both values present, delegates to
// the builtin `int.compare` from inside the impl body. The catch value is
// compared inline (a catch result stored in a test-block `let` stays boxed).
test "comparable_user_class_fallible_happy_path" {
    assert.is_true((ComparableFlaky { value: 1 }.compare(ComparableFlaky { value: 2 }) catch (e) {
        ComparableCompareError => 99
    }) < 0)
}

test "comparable_user_class_custom_cmperror_propagates" {
    let msg = {
        let v = ComparableFlaky { value: 1 }.compare(ComparableFlaky { value: null });
        "no throw"
    } catch (e) {
        ComparableCompareError => e.message
    };
    assert.is_true(msg.matches("cannot compare null"))
}

// ─── Sortable: `sort` (Phase 3 — new path alongside the old `sort`) ─────────
//
// Parity with the Phase 0 `sort()` corpus. These exercise `sort()` on *local*
// array receivers (`xs.sort()`).
//
// NB: these run the sort on a *local* receiver rather than through a
// `sort_ints(xs)` helper. Passing a test-block `let` local to a function
// delivers it as `Null` inside the callee — a pre-existing VM quirk of `test`
// blocks (see the `arrays.baml` header note), unrelated to sort. Param-receiver
// dispatch itself works in real functions (Rust `phase3_*` tests). The helper
// functions below are still *declared* (and compile) to pin the `throws never`
// normalization for parameter receivers at compile time.

function sort_ints(xs: int[]) -> int[] throws never { xs.sort() }
function sort_strings(xs: string[]) -> string[] throws never { xs.sort() }
function sort_bigints(xs: bigint[]) -> bigint[] throws never { xs.sort() }

test "sort_ints_in_place_returns_self" {
    let xs = [3, 1, 2];
    let result = xs.sort();
    let expected = [1, 2, 3];
    assert.is_true(result == xs);
    assert.is_true(baml.deep_equals(xs, expected))
}

test "sort_empty_is_noop" {
    let xs: int[] = [];
    let result = xs.sort();
    let expected: int[] = [];
    assert.is_true(result == xs);
    assert.is_true(baml.deep_equals(xs, expected))
}

test "sort_duplicates" {
    let xs = [3, 1, 2, 1, 3];
    xs.sort();
    let expected = [1, 1, 2, 3, 3];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "sort_strings_match_sort" {
    let xs = ["b", "a", "aa"];
    xs.sort();
    let expected = ["a", "aa", "b"];
    assert.is_true(baml.deep_equals(xs, expected))
}

test "sort_bigints_match_sort" {
    let xs = [3n, 1n, 2n];
    xs.sort();
    let expected = [1n, 2n, 3n];
    assert.is_true(baml.deep_equals(xs, expected))
}

// float's SortError is `never` (total_cmp decision) — infallible like int.
function sort_floats(xs: float[]) -> float[] throws never {
    xs.sort()
}

test "sort_floats_match_sort" {
    let xs = [2.5, 0.5, 1.5];
    xs.sort();
    let expected = [0.5, 1.5, 2.5];
    assert.is_true(baml.deep_equals(xs, expected))
}

// DECISION TEST (02-plan Phase 3): NaN no longer throws — `total_cmp` is a
// total order over all doubles, so NaN sorts deterministically after +inf.
test "sort_floats_nan_sorts_last" {
    let xs = [1.0, float.nan(), 0.5];
    xs.sort();
    assert.is_true(baml.deep_equals(xs.length(), 3));
    assert.equal(xs.at(0), 0.5);
    assert.equal(xs.at(1), 1.0);
    // NaN != NaN under `==`; assert positionally via is_nan.
    let last = xs.at(2);
    match (last) {
        null => assert.is_true(false)
        let f: float => assert.is_true(f.is_nan())
    }
}

// User class with an infallible (`never`) CompareError sorts by its compare —
// stably, in place, returning self.
class SortableItem {
    rank int
    label string
    implements baml.Comparable {
        type CompareError = never
        function compare(self, other: Self) -> int throws never {
            self.rank.compare(other.rank)
        }
    }
}

// `sort_items` is declared (and compiles) to pin the `throws never`
// normalization for an infallible user `Comparable` at compile time; the
// runtime tests below dispatch on local receivers (see the param-receiver
// limitation note above).
function sort_items(xs: SortableItem[]) -> SortableItem[] throws never {
    xs.sort()
}

test "sort_user_class_sorts_by_compare" {
    let xs = [
        SortableItem { rank: 3, label: "c" },
        SortableItem { rank: 1, label: "a" },
        SortableItem { rank: 2, label: "b" }
    ];
    let result = xs.sort();
    assert.is_true(result == xs);
    let labels = xs.map((x: SortableItem) -> string { x.label });
    let expected = ["a", "b", "c"];
    assert.is_true(baml.deep_equals(labels, expected))
}

test "sort_user_class_is_stable" {
    let xs = [
        SortableItem { rank: 2, label: "a" },
        SortableItem { rank: 1, label: "b" },
        SortableItem { rank: 2, label: "c" },
        SortableItem { rank: 1, label: "d" }
    ];
    xs.sort();
    let labels = xs.map((x: SortableItem) -> string { x.label });
    let expected = ["b", "d", "a", "c"];
    assert.is_true(baml.deep_equals(labels, expected))
}

// Fallible comparator (CompareError = ComparableCompareError): the call site must
// handle it, and a throw mid-sort leaves the array in pre-sort state.
function sort_flaky(xs: ComparableFlaky[]) -> ComparableFlaky[] throws ComparableCompareError {
    xs.sort()
}

test "sort_fallible_comparator_throw_rolls_back" {
    let xs = [
        ComparableFlaky { value: 3 },
        ComparableFlaky { value: null },
        ComparableFlaky { value: 1 }
    ];
    let msg = {
        sort_flaky(xs);
        "no throw"
    } catch (e) {
        ComparableCompareError => e.message
    };
    assert.is_true(msg.matches("cannot compare null"));
    // rollback: original order intact
    let vals = xs.map((x: ComparableFlaky) -> int {
        if let v: int = x.value { v } else { -1 }
    });
    let expected = [3, -1, 1];
    assert.is_true(baml.deep_equals(vals, expected))
}

test "sort_fallible_comparator_happy_path" {
    let xs = [
        ComparableFlaky { value: 3 },
        ComparableFlaky { value: 1 },
        ComparableFlaky { value: 2 }
    ];
    // No-throw contract: all values present, so the comparator never fails.
    // Asserted inline — a catch result stored in a test-block `let` stays
    // boxed (same VM quirk as above) and `== false` would reject the box.
    assert.is_true(({
        sort_flaky(xs);
        false
    } catch (e) {
        ComparableCompareError => true
    }) == false);
    let vals = xs.map((x: ComparableFlaky) -> int {
        if let v: int = x.value { v } else { -1 }
    });
    let expected = [1, 2, 3];
    assert.is_true(baml.deep_equals(vals, expected))
}

// Generic propagation: a function over `U extends Comparable` may call
// `xs.sort()`, propagating `U.CompareError` symbolically; the test routes
// through it to pin the runtime instantiation too. The array is built in a
// wrapper *function* — a test-block local passed to a generic function
// arrives boxed (the VM quirk above) and the native sort rejects the box.
function sort_generic<U extends baml.Comparable>(xs: U[]) -> U[] throws U.CompareError {
    xs.sort()
}

function run_sort_generic_ints() -> int[] throws never {
    let xs = [3, 1, 2];
    sort_generic(xs)
}

test "sort_generic_propagation" {
    let result = run_sort_generic_ints();
    let expected = [1, 2, 3];
    assert.is_true(baml.deep_equals(result, expected))
}

// ─── out-of-body `implements baml.Comparable for <class>` ───────────────────
//
// The impl methods register under the synthetic `Comparable$for$<class>`
// name (as spelled at the impl site), in this file's namespace. Pins that
// `_compare_shim`'s runtime dispatch finds them for class elements — the
// namespaced complement to the flat-package Rust test
// (`sort_user_class_with_out_of_body_comparable_impl`).
class OutOfBodyScore {
    points int
}

implements baml.Comparable for OutOfBodyScore {
    type CompareError = never
    function compare(self, other: Self) -> int throws never {
        self.points.compare(other.points)
    }
}

test "sort_user_class_out_of_body_impl" {
    let xs = [
        OutOfBodyScore { points: 3 },
        OutOfBodyScore { points: 1 },
        OutOfBodyScore { points: 2 }
    ];
    xs.sort();
    let pts = xs.map((x: OutOfBodyScore) -> int { x.points });
    let expected = [1, 2, 3];
    assert.is_true(baml.deep_equals(pts, expected))
}
