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
    assert.is_true(msg.includes("cannot compare null"))
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
    assert.equal(xs, expected)
}

test "sort_empty_is_noop" {
    let xs: int[] = [];
    let result = xs.sort();
    let expected: int[] = [];
    assert.is_true(result == xs);
    assert.equal(xs, expected)
}

test "sort_duplicates" {
    let xs = [3, 1, 2, 1, 3];
    xs.sort();
    let expected = [1, 1, 2, 3, 3];
    assert.equal(xs, expected)
}

test "sort_strings_match_sort" {
    let xs = ["b", "a", "aa"];
    xs.sort();
    let expected = ["a", "aa", "b"];
    assert.equal(xs, expected)
}

test "sort_bigints_match_sort" {
    let xs = [3n, 1n, 2n];
    xs.sort();
    let expected = [1n, 2n, 3n];
    assert.equal(xs, expected)
}

// float's SortError is `never` (total_cmp decision) — infallible like int.
function sort_floats(xs: float[]) -> float[] throws never {
    xs.sort()
}

test "sort_floats_match_sort" {
    let xs = [2.5, 0.5, 1.5];
    xs.sort();
    let expected = [0.5, 1.5, 2.5];
    assert.equal(xs, expected)
}

// DECISION TEST (02-plan Phase 3): NaN no longer throws — `total_cmp` is a
// total order over all doubles, so NaN sorts deterministically after +inf.
test "sort_floats_nan_sorts_last" {
    let xs = [1.0, float.nan(), 0.5];
    xs.sort();
    assert.equal(xs.length(), 3);
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
    assert.equal(labels, expected)
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
    assert.equal(labels, expected)
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
    assert.is_true(msg.includes("cannot compare null"));
    // rollback: original order intact
    let vals = xs.map((x: ComparableFlaky) -> int {
        if let v: int = x.value { v } else { -1 }
    });
    let expected = [3, -1, 1];
    assert.equal(vals, expected)
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
    assert.equal(vals, expected)
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
    assert.equal(result, expected)
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
    assert.equal(pts, expected)
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// interfaces_sort_comparable.baml:1:1 (comment) len=74 "// Tests for the `Comparable` interface (stdlib `comparable.baml`) and the"
// interfaces_sort_comparable.baml:2:1 (comment) len=50 "// `Sortable`-based array sort built on top of it."
// interfaces_sort_comparable.baml:3:1 (comment) len=2 "//"
// interfaces_sort_comparable.baml:4:1 (comment) len=71 "// Phase 2 of thoughts/sam-projects/array-sort/01b-option-5-tdd-plan.md"
// interfaces_sort_comparable.baml:5:1 (comment) len=67 "// (float semantics updated by the 02 follow-up): builtin impls for"
// interfaces_sort_comparable.baml:6:1 (comment) len=78 "// int/float/bigint/string, `type CompareError = never` for all four — float"
// interfaces_sort_comparable.baml:7:1 (comment) len=73 "// orders by IEEE 754 totalOrder (`f64::total_cmp`), so NaN has a defined"
// interfaces_sort_comparable.baml:8:1 (comment) len=39 "// position and `compare` never throws."
// interfaces_sort_comparable.baml:10:1 (comment) len=77 "// NB on shape: receivers are *annotated* test-block locals and arguments are"
// interfaces_sort_comparable.baml:11:1 (comment) len=75 "// literals (or inline calls). The annotation widens the receiver's literal"
// interfaces_sort_comparable.baml:12:1 (comment) len=74 "// type — `(5).compare(3)` would bind `Self` to the literal type `5` and"
// interfaces_sort_comparable.baml:13:1 (comment) len=78 "// reject `3` — and literal arguments widen into `Self` fine. Arguments must"
// interfaces_sort_comparable.baml:14:1 (comment) len=72 "// not be test-block locals: a local passed as the `other` argument of a"
// interfaces_sort_comparable.baml:15:1 (comment) len=76 "// two-`Self` interface method arrives boxed in a `test` block (pre-existing"
// interfaces_sort_comparable.baml:16:1 (comment) len=73 "// VM quirk, see the user-class section note below) and the comparison op"
// interfaces_sort_comparable.baml:17:1 (comment) len=56 "// rejects the box. Receiver-position locals unbox fine."
// interfaces_sort_comparable.baml:19:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:19:6 (string) len=24 "\"comparable_int_compare\""
// interfaces_sort_comparable.baml:20:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:20:9 (variable) [declaration] len=4 "five"
// interfaces_sort_comparable.baml:20:15 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:20:19 (operator) len=1 "="
// interfaces_sort_comparable.baml:20:21 (number) len=1 "5"
// interfaces_sort_comparable.baml:21:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:21:9 (variable) [declaration] len=5 "three"
// interfaces_sort_comparable.baml:21:16 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:21:20 (operator) len=1 "="
// interfaces_sort_comparable.baml:21:22 (number) len=1 "3"
// interfaces_sort_comparable.baml:22:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:22:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:22:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:22:20 (variable) len=4 "five"
// interfaces_sort_comparable.baml:22:25 (method) len=7 "compare"
// interfaces_sort_comparable.baml:22:33 (number) len=1 "3"
// interfaces_sort_comparable.baml:22:36 (operator) len=1 ">"
// interfaces_sort_comparable.baml:22:38 (number) len=1 "0"
// interfaces_sort_comparable.baml:23:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:23:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:23:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:23:20 (variable) len=5 "three"
// interfaces_sort_comparable.baml:23:26 (method) len=7 "compare"
// interfaces_sort_comparable.baml:23:34 (number) len=1 "5"
// interfaces_sort_comparable.baml:23:37 (operator) len=1 "<"
// interfaces_sort_comparable.baml:23:39 (number) len=1 "0"
// interfaces_sort_comparable.baml:24:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:24:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:24:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:24:20 (variable) len=5 "three"
// interfaces_sort_comparable.baml:24:26 (method) len=7 "compare"
// interfaces_sort_comparable.baml:24:34 (number) len=1 "3"
// interfaces_sort_comparable.baml:24:37 (operator) len=2 "=="
// interfaces_sort_comparable.baml:24:40 (number) len=1 "0"
// interfaces_sort_comparable.baml:27:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:27:6 (string) len=27 "\"comparable_string_compare\""
// interfaces_sort_comparable.baml:28:5 (comment) len=70 "// Annotated receivers widen the literal types (\"a\".compare(\"b\") would"
// interfaces_sort_comparable.baml:29:5 (comment) len=73 "// bind `Self` to the literal type \"a\" and reject \"b\"); literal arguments"
// interfaces_sort_comparable.baml:30:5 (comment) len=26 "// widen into `Self` fine."
// interfaces_sort_comparable.baml:31:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:31:9 (variable) [declaration] len=1 "a"
// interfaces_sort_comparable.baml:31:12 (type) [defaultLibrary] len=6 "string"
// interfaces_sort_comparable.baml:31:19 (operator) len=1 "="
// interfaces_sort_comparable.baml:31:21 (string) len=3 "\"a\""
// interfaces_sort_comparable.baml:32:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:32:9 (variable) [declaration] len=1 "b"
// interfaces_sort_comparable.baml:32:12 (type) [defaultLibrary] len=6 "string"
// interfaces_sort_comparable.baml:32:19 (operator) len=1 "="
// interfaces_sort_comparable.baml:32:21 (string) len=3 "\"b\""
// interfaces_sort_comparable.baml:33:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:33:9 (variable) [declaration] len=2 "aa"
// interfaces_sort_comparable.baml:33:13 (type) [defaultLibrary] len=6 "string"
// interfaces_sort_comparable.baml:33:20 (operator) len=1 "="
// interfaces_sort_comparable.baml:33:22 (string) len=4 "\"aa\""
// interfaces_sort_comparable.baml:34:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:34:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:34:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:34:20 (variable) len=1 "a"
// interfaces_sort_comparable.baml:34:22 (method) len=7 "compare"
// interfaces_sort_comparable.baml:34:30 (string) len=3 "\"b\""
// interfaces_sort_comparable.baml:34:35 (operator) len=1 "<"
// interfaces_sort_comparable.baml:34:37 (number) len=1 "0"
// interfaces_sort_comparable.baml:35:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:35:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:35:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:35:20 (variable) len=1 "b"
// interfaces_sort_comparable.baml:35:22 (method) len=7 "compare"
// interfaces_sort_comparable.baml:35:30 (string) len=3 "\"a\""
// interfaces_sort_comparable.baml:35:35 (operator) len=1 ">"
// interfaces_sort_comparable.baml:35:37 (number) len=1 "0"
// interfaces_sort_comparable.baml:36:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:36:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:36:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:36:20 (variable) len=2 "aa"
// interfaces_sort_comparable.baml:36:23 (method) len=7 "compare"
// interfaces_sort_comparable.baml:36:31 (string) len=3 "\"a\""
// interfaces_sort_comparable.baml:36:36 (operator) len=1 ">"
// interfaces_sort_comparable.baml:36:38 (number) len=1 "0"
// interfaces_sort_comparable.baml:37:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:37:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:37:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:37:20 (variable) len=1 "a"
// interfaces_sort_comparable.baml:37:22 (method) len=7 "compare"
// interfaces_sort_comparable.baml:37:30 (string) len=3 "\"a\""
// interfaces_sort_comparable.baml:37:35 (operator) len=2 "=="
// interfaces_sort_comparable.baml:37:38 (number) len=1 "0"
// interfaces_sort_comparable.baml:40:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:40:6 (string) len=27 "\"comparable_bigint_compare\""
// interfaces_sort_comparable.baml:41:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:41:9 (variable) [declaration] len=3 "one"
// interfaces_sort_comparable.baml:41:14 (type) [defaultLibrary] len=6 "bigint"
// interfaces_sort_comparable.baml:41:21 (operator) len=1 "="
// interfaces_sort_comparable.baml:41:23 (number) len=2 "1n"
// interfaces_sort_comparable.baml:42:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:42:9 (variable) [declaration] len=3 "two"
// interfaces_sort_comparable.baml:42:14 (type) [defaultLibrary] len=6 "bigint"
// interfaces_sort_comparable.baml:42:21 (operator) len=1 "="
// interfaces_sort_comparable.baml:42:23 (number) len=2 "2n"
// interfaces_sort_comparable.baml:43:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:43:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:43:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:43:20 (variable) len=3 "two"
// interfaces_sort_comparable.baml:43:24 (method) len=7 "compare"
// interfaces_sort_comparable.baml:43:32 (number) len=2 "1n"
// interfaces_sort_comparable.baml:43:36 (operator) len=1 ">"
// interfaces_sort_comparable.baml:43:38 (number) len=1 "0"
// interfaces_sort_comparable.baml:44:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:44:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:44:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:44:20 (variable) len=3 "one"
// interfaces_sort_comparable.baml:44:24 (method) len=7 "compare"
// interfaces_sort_comparable.baml:44:32 (number) len=2 "2n"
// interfaces_sort_comparable.baml:44:36 (operator) len=1 "<"
// interfaces_sort_comparable.baml:44:38 (number) len=1 "0"
// interfaces_sort_comparable.baml:45:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:45:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:45:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:45:20 (variable) len=3 "two"
// interfaces_sort_comparable.baml:45:24 (method) len=7 "compare"
// interfaces_sort_comparable.baml:45:32 (number) len=2 "2n"
// interfaces_sort_comparable.baml:45:36 (operator) len=2 "=="
// interfaces_sort_comparable.baml:45:39 (number) len=1 "0"
// interfaces_sort_comparable.baml:48:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:48:6 (string) len=26 "\"comparable_float_compare\""
// interfaces_sort_comparable.baml:49:5 (comment) len=61 "// Same literal-type widening dance as the string test above."
// interfaces_sort_comparable.baml:50:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:50:9 (variable) [declaration] len=2 "lo"
// interfaces_sort_comparable.baml:50:13 (type) [defaultLibrary] len=5 "float"
// interfaces_sort_comparable.baml:50:19 (operator) len=1 "="
// interfaces_sort_comparable.baml:50:21 (number) len=3 "1.5"
// interfaces_sort_comparable.baml:51:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:51:9 (variable) [declaration] len=2 "hi"
// interfaces_sort_comparable.baml:51:13 (type) [defaultLibrary] len=5 "float"
// interfaces_sort_comparable.baml:51:19 (operator) len=1 "="
// interfaces_sort_comparable.baml:51:21 (number) len=3 "2.5"
// interfaces_sort_comparable.baml:52:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:52:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:52:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:52:20 (variable) len=2 "hi"
// interfaces_sort_comparable.baml:52:23 (method) len=7 "compare"
// interfaces_sort_comparable.baml:52:31 (number) len=3 "1.5"
// interfaces_sort_comparable.baml:52:36 (operator) len=1 ">"
// interfaces_sort_comparable.baml:52:38 (number) len=1 "0"
// interfaces_sort_comparable.baml:53:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:53:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:53:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:53:20 (variable) len=2 "lo"
// interfaces_sort_comparable.baml:53:23 (method) len=7 "compare"
// interfaces_sort_comparable.baml:53:31 (number) len=3 "2.5"
// interfaces_sort_comparable.baml:53:36 (operator) len=1 "<"
// interfaces_sort_comparable.baml:53:38 (number) len=1 "0"
// interfaces_sort_comparable.baml:54:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:54:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:54:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:54:20 (variable) len=2 "lo"
// interfaces_sort_comparable.baml:54:23 (method) len=7 "compare"
// interfaces_sort_comparable.baml:54:31 (number) len=3 "1.5"
// interfaces_sort_comparable.baml:54:36 (operator) len=2 "=="
// interfaces_sort_comparable.baml:54:39 (number) len=1 "0"
// interfaces_sort_comparable.baml:57:1 (comment) len=72 "// DECISION TEST (02-plan Phase 3, reverses the 01b NaN decision): float"
// interfaces_sort_comparable.baml:58:1 (comment) len=75 "// comparison no longer throws on NaN — `total_cmp` orders NaN after +inf"
// interfaces_sort_comparable.baml:59:1 (comment) len=45 "// (`… < +inf < +NaN`) and equal to itself."
// interfaces_sort_comparable.baml:60:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:60:6 (string) len=34 "\"comparable_float_nan_total_order\""
// interfaces_sort_comparable.baml:61:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:61:9 (variable) [declaration] len=3 "one"
// interfaces_sort_comparable.baml:61:14 (type) [defaultLibrary] len=5 "float"
// interfaces_sort_comparable.baml:61:20 (operator) len=1 "="
// interfaces_sort_comparable.baml:61:22 (number) len=3 "1.0"
// interfaces_sort_comparable.baml:62:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:62:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:62:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:62:20 (variable) len=3 "one"
// interfaces_sort_comparable.baml:62:24 (method) len=7 "compare"
// interfaces_sort_comparable.baml:62:32 (type) [defaultLibrary] len=5 "float"
// interfaces_sort_comparable.baml:62:38 (method) len=3 "nan"
// interfaces_sort_comparable.baml:62:45 (operator) len=1 "<"
// interfaces_sort_comparable.baml:62:47 (number) len=1 "0"
// interfaces_sort_comparable.baml:63:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:63:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:63:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:63:20 (type) [defaultLibrary] len=5 "float"
// interfaces_sort_comparable.baml:63:26 (method) len=3 "nan"
// interfaces_sort_comparable.baml:63:32 (method) len=7 "compare"
// interfaces_sort_comparable.baml:63:40 (number) len=3 "1.0"
// interfaces_sort_comparable.baml:63:45 (operator) len=1 ">"
// interfaces_sort_comparable.baml:63:47 (number) len=1 "0"
// interfaces_sort_comparable.baml:64:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:64:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:64:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:64:20 (type) [defaultLibrary] len=5 "float"
// interfaces_sort_comparable.baml:64:26 (method) len=3 "nan"
// interfaces_sort_comparable.baml:64:32 (method) len=7 "compare"
// interfaces_sort_comparable.baml:64:40 (type) [defaultLibrary] len=5 "float"
// interfaces_sort_comparable.baml:64:46 (method) len=3 "inf"
// interfaces_sort_comparable.baml:64:53 (operator) len=1 ">"
// interfaces_sort_comparable.baml:64:55 (number) len=1 "0"
// interfaces_sort_comparable.baml:65:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:65:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:65:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:65:20 (type) [defaultLibrary] len=5 "float"
// interfaces_sort_comparable.baml:65:26 (method) len=3 "nan"
// interfaces_sort_comparable.baml:65:32 (method) len=7 "compare"
// interfaces_sort_comparable.baml:65:40 (type) [defaultLibrary] len=5 "float"
// interfaces_sort_comparable.baml:65:46 (method) len=3 "nan"
// interfaces_sort_comparable.baml:65:53 (operator) len=2 "=="
// interfaces_sort_comparable.baml:65:56 (number) len=1 "0"
// interfaces_sort_comparable.baml:68:1 (comment) len=186 "// ─── user classes opting in ──────────────────────────────────────────────────"
// interfaces_sort_comparable.baml:69:1 (comment) len=2 "//"
// interfaces_sort_comparable.baml:70:1 (comment) len=78 "// KNOWN VM BUG (pre-existing, not Comparable-specific): a class instance read"
// interfaces_sort_comparable.baml:71:1 (comment) len=77 "// back from a *test-block local* stays boxed, and passing a boxed value to a"
// interfaces_sort_comparable.baml:72:1 (comment) len=78 "// two-`Self`-param interface method (`compare(self, other: Self)`) panics the"
// interfaces_sort_comparable.baml:73:1 (comment) len=73 "// VM with \"unreachable code executed\". The same calls work from ordinary"
// interfaces_sort_comparable.baml:74:1 (comment) len=73 "// functions. Workaround used throughout: construct instances at the call"
// interfaces_sort_comparable.baml:75:1 (comment) len=59 "// boundary (inline arguments), never via test-block `let`."
// interfaces_sort_comparable.baml:77:1 (comment) len=74 "// Total-order user type: binds `type CompareError = never` and writes the"
// interfaces_sort_comparable.baml:78:1 (comment) len=77 "// matching `throws never` (an omitted `throws` is inferred, not `never`, and"
// interfaces_sort_comparable.baml:79:1 (comment) len=28 "// fails conformance E0120)."
// interfaces_sort_comparable.baml:80:1 (keyword) len=5 "class"
// interfaces_sort_comparable.baml:80:7 (class) [declaration] len=15 "ComparableScore"
// interfaces_sort_comparable.baml:81:5 (property) [declaration] len=6 "points"
// interfaces_sort_comparable.baml:81:12 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:82:5 (keyword) len=10 "implements"
// interfaces_sort_comparable.baml:82:16 (namespace) [defaultLibrary] len=4 "baml"
// interfaces_sort_comparable.baml:82:20 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:82:21 (interface) [defaultLibrary] len=10 "Comparable"
// interfaces_sort_comparable.baml:83:9 (keyword) len=4 "type"
// interfaces_sort_comparable.baml:83:14 (type) [declaration] len=12 "CompareError"
// interfaces_sort_comparable.baml:83:27 (operator) len=1 "="
// interfaces_sort_comparable.baml:83:29 (type) [defaultLibrary] len=5 "never"
// interfaces_sort_comparable.baml:84:9 (keyword) len=8 "function"
// interfaces_sort_comparable.baml:84:18 (method) [declaration] len=7 "compare"
// interfaces_sort_comparable.baml:84:26 (parameter) [declaration] len=4 "self"
// interfaces_sort_comparable.baml:84:32 (parameter) [declaration] len=5 "other"
// interfaces_sort_comparable.baml:84:39 (type) len=4 "Self"
// interfaces_sort_comparable.baml:84:48 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:84:52 (keyword) len=6 "throws"
// interfaces_sort_comparable.baml:84:59 (type) [defaultLibrary] len=5 "never"
// interfaces_sort_comparable.baml:85:13 (parameter) len=4 "self"
// interfaces_sort_comparable.baml:85:18 (property) len=6 "points"
// interfaces_sort_comparable.baml:85:25 (method) len=7 "compare"
// interfaces_sort_comparable.baml:85:33 (parameter) len=5 "other"
// interfaces_sort_comparable.baml:85:39 (property) len=6 "points"
// interfaces_sort_comparable.baml:90:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:90:6 (string) len=49 "\"comparable_user_class_default_cmperror_is_never\""
// interfaces_sort_comparable.baml:91:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:91:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:91:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:91:20 (class) len=15 "ComparableScore"
// interfaces_sort_comparable.baml:91:38 (property) len=6 "points"
// interfaces_sort_comparable.baml:91:46 (number) len=1 "9"
// interfaces_sort_comparable.baml:91:50 (method) len=7 "compare"
// interfaces_sort_comparable.baml:91:58 (class) len=15 "ComparableScore"
// interfaces_sort_comparable.baml:91:76 (property) len=6 "points"
// interfaces_sort_comparable.baml:91:84 (number) len=1 "1"
// interfaces_sort_comparable.baml:91:89 (operator) len=1 ">"
// interfaces_sort_comparable.baml:91:91 (number) len=1 "0"
// interfaces_sort_comparable.baml:92:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:92:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:92:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:92:20 (class) len=15 "ComparableScore"
// interfaces_sort_comparable.baml:92:38 (property) len=6 "points"
// interfaces_sort_comparable.baml:92:46 (number) len=1 "1"
// interfaces_sort_comparable.baml:92:50 (method) len=7 "compare"
// interfaces_sort_comparable.baml:92:58 (class) len=15 "ComparableScore"
// interfaces_sort_comparable.baml:92:76 (property) len=6 "points"
// interfaces_sort_comparable.baml:92:84 (number) len=1 "9"
// interfaces_sort_comparable.baml:92:89 (operator) len=1 "<"
// interfaces_sort_comparable.baml:92:91 (number) len=1 "0"
// interfaces_sort_comparable.baml:93:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:93:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:93:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:93:20 (class) len=15 "ComparableScore"
// interfaces_sort_comparable.baml:93:38 (property) len=6 "points"
// interfaces_sort_comparable.baml:93:46 (number) len=1 "1"
// interfaces_sort_comparable.baml:93:50 (method) len=7 "compare"
// interfaces_sort_comparable.baml:93:58 (class) len=15 "ComparableScore"
// interfaces_sort_comparable.baml:93:76 (property) len=6 "points"
// interfaces_sort_comparable.baml:93:84 (number) len=1 "1"
// interfaces_sort_comparable.baml:93:89 (operator) len=2 "=="
// interfaces_sort_comparable.baml:93:92 (number) len=1 "0"
// interfaces_sort_comparable.baml:96:1 (comment) len=86 "// Fallible comparator: binds `type CompareError = ComparableCompareError` and throws."
// interfaces_sort_comparable.baml:97:1 (keyword) len=5 "class"
// interfaces_sort_comparable.baml:97:7 (class) [declaration] len=22 "ComparableCompareError"
// interfaces_sort_comparable.baml:98:5 (property) [declaration] len=7 "message"
// interfaces_sort_comparable.baml:98:13 (type) [defaultLibrary] len=6 "string"
// interfaces_sort_comparable.baml:101:1 (keyword) len=5 "class"
// interfaces_sort_comparable.baml:101:7 (class) [declaration] len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:102:5 (property) [declaration] len=5 "value"
// interfaces_sort_comparable.baml:102:11 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:103:5 (keyword) len=10 "implements"
// interfaces_sort_comparable.baml:103:16 (namespace) [defaultLibrary] len=4 "baml"
// interfaces_sort_comparable.baml:103:20 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:103:21 (interface) [defaultLibrary] len=10 "Comparable"
// interfaces_sort_comparable.baml:104:9 (keyword) len=4 "type"
// interfaces_sort_comparable.baml:104:14 (type) [declaration] len=12 "CompareError"
// interfaces_sort_comparable.baml:104:27 (operator) len=1 "="
// interfaces_sort_comparable.baml:104:29 (class) len=22 "ComparableCompareError"
// interfaces_sort_comparable.baml:106:9 (keyword) len=8 "function"
// interfaces_sort_comparable.baml:106:18 (method) [declaration] len=7 "compare"
// interfaces_sort_comparable.baml:106:26 (parameter) [declaration] len=4 "self"
// interfaces_sort_comparable.baml:106:32 (parameter) [declaration] len=5 "other"
// interfaces_sort_comparable.baml:106:39 (type) len=4 "Self"
// interfaces_sort_comparable.baml:106:48 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:106:52 (keyword) len=6 "throws"
// interfaces_sort_comparable.baml:106:59 (class) len=22 "ComparableCompareError"
// interfaces_sort_comparable.baml:107:13 (keyword) len=2 "if"
// interfaces_sort_comparable.baml:107:16 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:107:20 (variable) [declaration] len=1 "a"
// interfaces_sort_comparable.baml:107:23 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:107:27 (operator) len=1 "="
// interfaces_sort_comparable.baml:107:29 (parameter) len=4 "self"
// interfaces_sort_comparable.baml:107:34 (property) len=5 "value"
// interfaces_sort_comparable.baml:108:17 (keyword) len=2 "if"
// interfaces_sort_comparable.baml:108:20 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:108:24 (variable) [declaration] len=1 "b"
// interfaces_sort_comparable.baml:108:27 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:108:31 (operator) len=1 "="
// interfaces_sort_comparable.baml:108:33 (parameter) len=5 "other"
// interfaces_sort_comparable.baml:108:39 (property) len=5 "value"
// interfaces_sort_comparable.baml:109:21 (keyword) len=6 "return"
// interfaces_sort_comparable.baml:109:28 (variable) len=1 "a"
// interfaces_sort_comparable.baml:109:30 (method) len=7 "compare"
// interfaces_sort_comparable.baml:109:38 (variable) len=1 "b"
// interfaces_sort_comparable.baml:112:13 (keyword) len=5 "throw"
// interfaces_sort_comparable.baml:112:19 (class) len=22 "ComparableCompareError"
// interfaces_sort_comparable.baml:112:44 (property) len=7 "message"
// interfaces_sort_comparable.baml:112:53 (string) len=21 "\"cannot compare null\""
// interfaces_sort_comparable.baml:117:1 (comment) len=74 "// The fallible comparator's happy path: both values present, delegates to"
// interfaces_sort_comparable.baml:118:1 (comment) len=74 "// the builtin `int.compare` from inside the impl body. The catch value is"
// interfaces_sort_comparable.baml:119:1 (comment) len=77 "// compared inline (a catch result stored in a test-block `let` stays boxed)."
// interfaces_sort_comparable.baml:120:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:120:6 (string) len=43 "\"comparable_user_class_fallible_happy_path\""
// interfaces_sort_comparable.baml:121:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:121:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:121:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:121:21 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:121:39 (property) len=5 "value"
// interfaces_sort_comparable.baml:121:46 (number) len=1 "1"
// interfaces_sort_comparable.baml:121:50 (method) len=7 "compare"
// interfaces_sort_comparable.baml:121:58 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:121:76 (property) len=5 "value"
// interfaces_sort_comparable.baml:121:83 (number) len=1 "2"
// interfaces_sort_comparable.baml:121:88 (keyword) len=5 "catch"
// interfaces_sort_comparable.baml:121:95 (parameter) [declaration] len=1 "e"
// interfaces_sort_comparable.baml:122:9 (class) len=22 "ComparableCompareError"
// interfaces_sort_comparable.baml:122:35 (number) len=2 "99"
// interfaces_sort_comparable.baml:123:8 (operator) len=1 "<"
// interfaces_sort_comparable.baml:123:10 (number) len=1 "0"
// interfaces_sort_comparable.baml:126:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:126:6 (string) len=50 "\"comparable_user_class_custom_cmperror_propagates\""
// interfaces_sort_comparable.baml:127:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:127:9 (variable) [declaration] len=3 "msg"
// interfaces_sort_comparable.baml:127:13 (operator) len=1 "="
// interfaces_sort_comparable.baml:128:9 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:128:13 (variable) [declaration] len=1 "v"
// interfaces_sort_comparable.baml:128:15 (operator) len=1 "="
// interfaces_sort_comparable.baml:128:17 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:128:35 (property) len=5 "value"
// interfaces_sort_comparable.baml:128:42 (number) len=1 "1"
// interfaces_sort_comparable.baml:128:46 (method) len=7 "compare"
// interfaces_sort_comparable.baml:128:54 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:128:72 (property) len=5 "value"
// interfaces_sort_comparable.baml:128:79 (type) [defaultLibrary] len=4 "null"
// interfaces_sort_comparable.baml:129:9 (string) len=10 "\"no throw\""
// interfaces_sort_comparable.baml:130:7 (keyword) len=5 "catch"
// interfaces_sort_comparable.baml:130:14 (parameter) [declaration] len=1 "e"
// interfaces_sort_comparable.baml:131:9 (class) len=22 "ComparableCompareError"
// interfaces_sort_comparable.baml:131:35 (parameter) len=1 "e"
// interfaces_sort_comparable.baml:131:37 (property) len=7 "message"
// interfaces_sort_comparable.baml:133:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:133:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:133:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:133:20 (variable) len=3 "msg"
// interfaces_sort_comparable.baml:133:24 (method) len=8 "includes"
// interfaces_sort_comparable.baml:133:33 (string) len=21 "\"cannot compare null\""
// interfaces_sort_comparable.baml:136:1 (comment) len=105 "// ─── Sortable: `sort` (Phase 3 — new path alongside the old `sort`) ─────────"
// interfaces_sort_comparable.baml:137:1 (comment) len=2 "//"
// interfaces_sort_comparable.baml:138:1 (comment) len=78 "// Parity with the Phase 0 `sort()` corpus. These exercise `sort()` on *local*"
// interfaces_sort_comparable.baml:139:1 (comment) len=33 "// array receivers (`xs.sort()`)."
// interfaces_sort_comparable.baml:140:1 (comment) len=2 "//"
// interfaces_sort_comparable.baml:141:1 (comment) len=69 "// NB: these run the sort on a *local* receiver rather than through a"
// interfaces_sort_comparable.baml:142:1 (comment) len=73 "// `sort_ints(xs)` helper. Passing a test-block `let` local to a function"
// interfaces_sort_comparable.baml:143:1 (comment) len=80 "// delivers it as `Null` inside the callee — a pre-existing VM quirk of `test`"
// interfaces_sort_comparable.baml:144:1 (comment) len=80 "// blocks (see the `arrays.baml` header note), unrelated to sort. Param-receiver"
// interfaces_sort_comparable.baml:145:1 (comment) len=78 "// dispatch itself works in real functions (Rust `phase3_*` tests). The helper"
// interfaces_sort_comparable.baml:146:1 (comment) len=79 "// functions below are still *declared* (and compile) to pin the `throws never`"
// interfaces_sort_comparable.baml:147:1 (comment) len=57 "// normalization for parameter receivers at compile time."
// interfaces_sort_comparable.baml:149:1 (keyword) len=8 "function"
// interfaces_sort_comparable.baml:149:10 (function) [declaration] len=9 "sort_ints"
// interfaces_sort_comparable.baml:149:20 (parameter) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:149:24 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:149:34 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:149:40 (keyword) len=6 "throws"
// interfaces_sort_comparable.baml:149:47 (type) [defaultLibrary] len=5 "never"
// interfaces_sort_comparable.baml:149:55 (parameter) len=2 "xs"
// interfaces_sort_comparable.baml:149:58 (method) len=4 "sort"
// interfaces_sort_comparable.baml:150:1 (keyword) len=8 "function"
// interfaces_sort_comparable.baml:150:10 (function) [declaration] len=12 "sort_strings"
// interfaces_sort_comparable.baml:150:23 (parameter) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:150:27 (type) [defaultLibrary] len=6 "string"
// interfaces_sort_comparable.baml:150:40 (type) [defaultLibrary] len=6 "string"
// interfaces_sort_comparable.baml:150:49 (keyword) len=6 "throws"
// interfaces_sort_comparable.baml:150:56 (type) [defaultLibrary] len=5 "never"
// interfaces_sort_comparable.baml:150:64 (parameter) len=2 "xs"
// interfaces_sort_comparable.baml:150:67 (method) len=4 "sort"
// interfaces_sort_comparable.baml:151:1 (keyword) len=8 "function"
// interfaces_sort_comparable.baml:151:10 (function) [declaration] len=12 "sort_bigints"
// interfaces_sort_comparable.baml:151:23 (parameter) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:151:27 (type) [defaultLibrary] len=6 "bigint"
// interfaces_sort_comparable.baml:151:40 (type) [defaultLibrary] len=6 "bigint"
// interfaces_sort_comparable.baml:151:49 (keyword) len=6 "throws"
// interfaces_sort_comparable.baml:151:56 (type) [defaultLibrary] len=5 "never"
// interfaces_sort_comparable.baml:151:64 (parameter) len=2 "xs"
// interfaces_sort_comparable.baml:151:67 (method) len=4 "sort"
// interfaces_sort_comparable.baml:153:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:153:6 (string) len=33 "\"sort_ints_in_place_returns_self\""
// interfaces_sort_comparable.baml:154:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:154:9 (variable) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:154:12 (operator) len=1 "="
// interfaces_sort_comparable.baml:154:15 (number) len=1 "3"
// interfaces_sort_comparable.baml:154:18 (number) len=1 "1"
// interfaces_sort_comparable.baml:154:21 (number) len=1 "2"
// interfaces_sort_comparable.baml:155:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:155:9 (variable) [declaration] len=6 "result"
// interfaces_sort_comparable.baml:155:16 (operator) len=1 "="
// interfaces_sort_comparable.baml:155:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:155:21 (method) len=4 "sort"
// interfaces_sort_comparable.baml:156:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:156:9 (variable) [declaration] len=8 "expected"
// interfaces_sort_comparable.baml:156:18 (operator) len=1 "="
// interfaces_sort_comparable.baml:156:21 (number) len=1 "1"
// interfaces_sort_comparable.baml:156:24 (number) len=1 "2"
// interfaces_sort_comparable.baml:156:27 (number) len=1 "3"
// interfaces_sort_comparable.baml:157:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:157:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:157:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:157:20 (variable) len=6 "result"
// interfaces_sort_comparable.baml:157:27 (operator) len=2 "=="
// interfaces_sort_comparable.baml:157:30 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:158:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:158:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:158:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:158:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:158:22 (variable) len=8 "expected"
// interfaces_sort_comparable.baml:161:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:161:6 (string) len=20 "\"sort_empty_is_noop\""
// interfaces_sort_comparable.baml:162:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:162:9 (variable) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:162:13 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:162:19 (operator) len=1 "="
// interfaces_sort_comparable.baml:163:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:163:9 (variable) [declaration] len=6 "result"
// interfaces_sort_comparable.baml:163:16 (operator) len=1 "="
// interfaces_sort_comparable.baml:163:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:163:21 (method) len=4 "sort"
// interfaces_sort_comparable.baml:164:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:164:9 (variable) [declaration] len=8 "expected"
// interfaces_sort_comparable.baml:164:19 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:164:25 (operator) len=1 "="
// interfaces_sort_comparable.baml:165:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:165:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:165:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:165:20 (variable) len=6 "result"
// interfaces_sort_comparable.baml:165:27 (operator) len=2 "=="
// interfaces_sort_comparable.baml:165:30 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:166:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:166:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:166:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:166:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:166:22 (variable) len=8 "expected"
// interfaces_sort_comparable.baml:169:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:169:6 (string) len=17 "\"sort_duplicates\""
// interfaces_sort_comparable.baml:170:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:170:9 (variable) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:170:12 (operator) len=1 "="
// interfaces_sort_comparable.baml:170:15 (number) len=1 "3"
// interfaces_sort_comparable.baml:170:18 (number) len=1 "1"
// interfaces_sort_comparable.baml:170:21 (number) len=1 "2"
// interfaces_sort_comparable.baml:170:24 (number) len=1 "1"
// interfaces_sort_comparable.baml:170:27 (number) len=1 "3"
// interfaces_sort_comparable.baml:171:5 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:171:8 (method) len=4 "sort"
// interfaces_sort_comparable.baml:172:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:172:9 (variable) [declaration] len=8 "expected"
// interfaces_sort_comparable.baml:172:18 (operator) len=1 "="
// interfaces_sort_comparable.baml:172:21 (number) len=1 "1"
// interfaces_sort_comparable.baml:172:24 (number) len=1 "1"
// interfaces_sort_comparable.baml:172:27 (number) len=1 "2"
// interfaces_sort_comparable.baml:172:30 (number) len=1 "3"
// interfaces_sort_comparable.baml:172:33 (number) len=1 "3"
// interfaces_sort_comparable.baml:173:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:173:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:173:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:173:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:173:22 (variable) len=8 "expected"
// interfaces_sort_comparable.baml:176:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:176:6 (string) len=25 "\"sort_strings_match_sort\""
// interfaces_sort_comparable.baml:177:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:177:9 (variable) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:177:12 (operator) len=1 "="
// interfaces_sort_comparable.baml:177:15 (string) len=3 "\"b\""
// interfaces_sort_comparable.baml:177:20 (string) len=3 "\"a\""
// interfaces_sort_comparable.baml:177:25 (string) len=4 "\"aa\""
// interfaces_sort_comparable.baml:178:5 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:178:8 (method) len=4 "sort"
// interfaces_sort_comparable.baml:179:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:179:9 (variable) [declaration] len=8 "expected"
// interfaces_sort_comparable.baml:179:18 (operator) len=1 "="
// interfaces_sort_comparable.baml:179:21 (string) len=3 "\"a\""
// interfaces_sort_comparable.baml:179:26 (string) len=4 "\"aa\""
// interfaces_sort_comparable.baml:179:32 (string) len=3 "\"b\""
// interfaces_sort_comparable.baml:180:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:180:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:180:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:180:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:180:22 (variable) len=8 "expected"
// interfaces_sort_comparable.baml:183:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:183:6 (string) len=25 "\"sort_bigints_match_sort\""
// interfaces_sort_comparable.baml:184:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:184:9 (variable) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:184:12 (operator) len=1 "="
// interfaces_sort_comparable.baml:184:15 (number) len=2 "3n"
// interfaces_sort_comparable.baml:184:19 (number) len=2 "1n"
// interfaces_sort_comparable.baml:184:23 (number) len=2 "2n"
// interfaces_sort_comparable.baml:185:5 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:185:8 (method) len=4 "sort"
// interfaces_sort_comparable.baml:186:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:186:9 (variable) [declaration] len=8 "expected"
// interfaces_sort_comparable.baml:186:18 (operator) len=1 "="
// interfaces_sort_comparable.baml:186:21 (number) len=2 "1n"
// interfaces_sort_comparable.baml:186:25 (number) len=2 "2n"
// interfaces_sort_comparable.baml:186:29 (number) len=2 "3n"
// interfaces_sort_comparable.baml:187:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:187:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:187:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:187:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:187:22 (variable) len=8 "expected"
// interfaces_sort_comparable.baml:190:1 (comment) len=77 "// float's SortError is `never` (total_cmp decision) — infallible like int."
// interfaces_sort_comparable.baml:191:1 (keyword) len=8 "function"
// interfaces_sort_comparable.baml:191:10 (function) [declaration] len=11 "sort_floats"
// interfaces_sort_comparable.baml:191:22 (parameter) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:191:26 (type) [defaultLibrary] len=5 "float"
// interfaces_sort_comparable.baml:191:38 (type) [defaultLibrary] len=5 "float"
// interfaces_sort_comparable.baml:191:46 (keyword) len=6 "throws"
// interfaces_sort_comparable.baml:191:53 (type) [defaultLibrary] len=5 "never"
// interfaces_sort_comparable.baml:192:5 (parameter) len=2 "xs"
// interfaces_sort_comparable.baml:192:8 (method) len=4 "sort"
// interfaces_sort_comparable.baml:195:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:195:6 (string) len=24 "\"sort_floats_match_sort\""
// interfaces_sort_comparable.baml:196:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:196:9 (variable) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:196:12 (operator) len=1 "="
// interfaces_sort_comparable.baml:196:15 (number) len=3 "2.5"
// interfaces_sort_comparable.baml:196:20 (number) len=3 "0.5"
// interfaces_sort_comparable.baml:196:25 (number) len=3 "1.5"
// interfaces_sort_comparable.baml:197:5 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:197:8 (method) len=4 "sort"
// interfaces_sort_comparable.baml:198:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:198:9 (variable) [declaration] len=8 "expected"
// interfaces_sort_comparable.baml:198:18 (operator) len=1 "="
// interfaces_sort_comparable.baml:198:21 (number) len=3 "0.5"
// interfaces_sort_comparable.baml:198:26 (number) len=3 "1.5"
// interfaces_sort_comparable.baml:198:31 (number) len=3 "2.5"
// interfaces_sort_comparable.baml:199:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:199:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:199:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:199:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:199:22 (variable) len=8 "expected"
// interfaces_sort_comparable.baml:202:1 (comment) len=77 "// DECISION TEST (02-plan Phase 3): NaN no longer throws — `total_cmp` is a"
// interfaces_sort_comparable.baml:203:1 (comment) len=75 "// total order over all doubles, so NaN sorts deterministically after +inf."
// interfaces_sort_comparable.baml:204:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:204:6 (string) len=28 "\"sort_floats_nan_sorts_last\""
// interfaces_sort_comparable.baml:205:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:205:9 (variable) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:205:12 (operator) len=1 "="
// interfaces_sort_comparable.baml:205:15 (number) len=3 "1.0"
// interfaces_sort_comparable.baml:205:20 (type) [defaultLibrary] len=5 "float"
// interfaces_sort_comparable.baml:205:26 (method) len=3 "nan"
// interfaces_sort_comparable.baml:205:33 (number) len=3 "0.5"
// interfaces_sort_comparable.baml:206:5 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:206:8 (method) len=4 "sort"
// interfaces_sort_comparable.baml:207:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:207:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:207:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:207:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:207:21 (method) len=6 "length"
// interfaces_sort_comparable.baml:207:31 (number) len=1 "3"
// interfaces_sort_comparable.baml:208:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:208:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:208:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:208:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:208:21 (method) len=2 "at"
// interfaces_sort_comparable.baml:208:24 (number) len=1 "0"
// interfaces_sort_comparable.baml:208:28 (number) len=3 "0.5"
// interfaces_sort_comparable.baml:209:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:209:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:209:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:209:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:209:21 (method) len=2 "at"
// interfaces_sort_comparable.baml:209:24 (number) len=1 "1"
// interfaces_sort_comparable.baml:209:28 (number) len=3 "1.0"
// interfaces_sort_comparable.baml:210:5 (comment) len=57 "// NaN != NaN under `==`; assert positionally via is_nan."
// interfaces_sort_comparable.baml:211:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:211:9 (variable) [declaration] len=4 "last"
// interfaces_sort_comparable.baml:211:14 (operator) len=1 "="
// interfaces_sort_comparable.baml:211:16 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:211:19 (method) len=2 "at"
// interfaces_sort_comparable.baml:211:22 (number) len=1 "2"
// interfaces_sort_comparable.baml:212:5 (keyword) len=5 "match"
// interfaces_sort_comparable.baml:212:12 (variable) len=4 "last"
// interfaces_sort_comparable.baml:213:9 (type) [defaultLibrary] len=4 "null"
// interfaces_sort_comparable.baml:213:17 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:213:23 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:213:24 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:213:32 (boolean) len=5 "false"
// interfaces_sort_comparable.baml:214:9 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:214:13 (variable) [declaration] len=1 "f"
// interfaces_sort_comparable.baml:214:16 (type) [defaultLibrary] len=5 "float"
// interfaces_sort_comparable.baml:214:25 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:214:31 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:214:32 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:214:40 (variable) len=1 "f"
// interfaces_sort_comparable.baml:214:42 (method) len=6 "is_nan"
// interfaces_sort_comparable.baml:218:1 (comment) len=80 "// User class with an infallible (`never`) CompareError sorts by its compare —"
// interfaces_sort_comparable.baml:219:1 (comment) len=36 "// stably, in place, returning self."
// interfaces_sort_comparable.baml:220:1 (keyword) len=5 "class"
// interfaces_sort_comparable.baml:220:7 (class) [declaration] len=12 "SortableItem"
// interfaces_sort_comparable.baml:221:5 (property) [declaration] len=4 "rank"
// interfaces_sort_comparable.baml:221:10 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:222:5 (property) [declaration] len=5 "label"
// interfaces_sort_comparable.baml:222:11 (type) [defaultLibrary] len=6 "string"
// interfaces_sort_comparable.baml:223:5 (keyword) len=10 "implements"
// interfaces_sort_comparable.baml:223:16 (namespace) [defaultLibrary] len=4 "baml"
// interfaces_sort_comparable.baml:223:20 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:223:21 (interface) [defaultLibrary] len=10 "Comparable"
// interfaces_sort_comparable.baml:224:9 (keyword) len=4 "type"
// interfaces_sort_comparable.baml:224:14 (type) [declaration] len=12 "CompareError"
// interfaces_sort_comparable.baml:224:27 (operator) len=1 "="
// interfaces_sort_comparable.baml:224:29 (type) [defaultLibrary] len=5 "never"
// interfaces_sort_comparable.baml:225:9 (keyword) len=8 "function"
// interfaces_sort_comparable.baml:225:18 (method) [declaration] len=7 "compare"
// interfaces_sort_comparable.baml:225:26 (parameter) [declaration] len=4 "self"
// interfaces_sort_comparable.baml:225:32 (parameter) [declaration] len=5 "other"
// interfaces_sort_comparable.baml:225:39 (type) len=4 "Self"
// interfaces_sort_comparable.baml:225:48 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:225:52 (keyword) len=6 "throws"
// interfaces_sort_comparable.baml:225:59 (type) [defaultLibrary] len=5 "never"
// interfaces_sort_comparable.baml:226:13 (parameter) len=4 "self"
// interfaces_sort_comparable.baml:226:18 (property) len=4 "rank"
// interfaces_sort_comparable.baml:226:23 (method) len=7 "compare"
// interfaces_sort_comparable.baml:226:31 (parameter) len=5 "other"
// interfaces_sort_comparable.baml:226:37 (property) len=4 "rank"
// interfaces_sort_comparable.baml:231:1 (comment) len=68 "// `sort_items` is declared (and compiles) to pin the `throws never`"
// interfaces_sort_comparable.baml:232:1 (comment) len=73 "// normalization for an infallible user `Comparable` at compile time; the"
// interfaces_sort_comparable.baml:233:1 (comment) len=74 "// runtime tests below dispatch on local receivers (see the param-receiver"
// interfaces_sort_comparable.baml:234:1 (comment) len=26 "// limitation note above)."
// interfaces_sort_comparable.baml:235:1 (keyword) len=8 "function"
// interfaces_sort_comparable.baml:235:10 (function) [declaration] len=10 "sort_items"
// interfaces_sort_comparable.baml:235:21 (parameter) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:235:25 (class) len=12 "SortableItem"
// interfaces_sort_comparable.baml:235:44 (class) len=12 "SortableItem"
// interfaces_sort_comparable.baml:235:59 (keyword) len=6 "throws"
// interfaces_sort_comparable.baml:235:66 (type) [defaultLibrary] len=5 "never"
// interfaces_sort_comparable.baml:236:5 (parameter) len=2 "xs"
// interfaces_sort_comparable.baml:236:8 (method) len=4 "sort"
// interfaces_sort_comparable.baml:239:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:239:6 (string) len=34 "\"sort_user_class_sorts_by_compare\""
// interfaces_sort_comparable.baml:240:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:240:9 (variable) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:240:12 (operator) len=1 "="
// interfaces_sort_comparable.baml:241:9 (class) len=12 "SortableItem"
// interfaces_sort_comparable.baml:241:24 (property) len=4 "rank"
// interfaces_sort_comparable.baml:241:30 (number) len=1 "3"
// interfaces_sort_comparable.baml:241:33 (property) len=5 "label"
// interfaces_sort_comparable.baml:241:40 (string) len=3 "\"c\""
// interfaces_sort_comparable.baml:242:9 (class) len=12 "SortableItem"
// interfaces_sort_comparable.baml:242:24 (property) len=4 "rank"
// interfaces_sort_comparable.baml:242:30 (number) len=1 "1"
// interfaces_sort_comparable.baml:242:33 (property) len=5 "label"
// interfaces_sort_comparable.baml:242:40 (string) len=3 "\"a\""
// interfaces_sort_comparable.baml:243:9 (class) len=12 "SortableItem"
// interfaces_sort_comparable.baml:243:24 (property) len=4 "rank"
// interfaces_sort_comparable.baml:243:30 (number) len=1 "2"
// interfaces_sort_comparable.baml:243:33 (property) len=5 "label"
// interfaces_sort_comparable.baml:243:40 (string) len=3 "\"b\""
// interfaces_sort_comparable.baml:245:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:245:9 (variable) [declaration] len=6 "result"
// interfaces_sort_comparable.baml:245:16 (operator) len=1 "="
// interfaces_sort_comparable.baml:245:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:245:21 (method) len=4 "sort"
// interfaces_sort_comparable.baml:246:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:246:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:246:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:246:20 (variable) len=6 "result"
// interfaces_sort_comparable.baml:246:27 (operator) len=2 "=="
// interfaces_sort_comparable.baml:246:30 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:247:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:247:9 (variable) [declaration] len=6 "labels"
// interfaces_sort_comparable.baml:247:16 (operator) len=1 "="
// interfaces_sort_comparable.baml:247:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:247:21 (method) len=3 "map"
// interfaces_sort_comparable.baml:247:26 (parameter) [declaration] len=1 "x"
// interfaces_sort_comparable.baml:247:29 (class) len=12 "SortableItem"
// interfaces_sort_comparable.baml:247:46 (type) [defaultLibrary] len=6 "string"
// interfaces_sort_comparable.baml:247:55 (parameter) len=1 "x"
// interfaces_sort_comparable.baml:247:57 (property) len=5 "label"
// interfaces_sort_comparable.baml:248:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:248:9 (variable) [declaration] len=8 "expected"
// interfaces_sort_comparable.baml:248:18 (operator) len=1 "="
// interfaces_sort_comparable.baml:248:21 (string) len=3 "\"a\""
// interfaces_sort_comparable.baml:248:26 (string) len=3 "\"b\""
// interfaces_sort_comparable.baml:248:31 (string) len=3 "\"c\""
// interfaces_sort_comparable.baml:249:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:249:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:249:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:249:18 (variable) len=6 "labels"
// interfaces_sort_comparable.baml:249:26 (variable) len=8 "expected"
// interfaces_sort_comparable.baml:252:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:252:6 (string) len=27 "\"sort_user_class_is_stable\""
// interfaces_sort_comparable.baml:253:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:253:9 (variable) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:253:12 (operator) len=1 "="
// interfaces_sort_comparable.baml:254:9 (class) len=12 "SortableItem"
// interfaces_sort_comparable.baml:254:24 (property) len=4 "rank"
// interfaces_sort_comparable.baml:254:30 (number) len=1 "2"
// interfaces_sort_comparable.baml:254:33 (property) len=5 "label"
// interfaces_sort_comparable.baml:254:40 (string) len=3 "\"a\""
// interfaces_sort_comparable.baml:255:9 (class) len=12 "SortableItem"
// interfaces_sort_comparable.baml:255:24 (property) len=4 "rank"
// interfaces_sort_comparable.baml:255:30 (number) len=1 "1"
// interfaces_sort_comparable.baml:255:33 (property) len=5 "label"
// interfaces_sort_comparable.baml:255:40 (string) len=3 "\"b\""
// interfaces_sort_comparable.baml:256:9 (class) len=12 "SortableItem"
// interfaces_sort_comparable.baml:256:24 (property) len=4 "rank"
// interfaces_sort_comparable.baml:256:30 (number) len=1 "2"
// interfaces_sort_comparable.baml:256:33 (property) len=5 "label"
// interfaces_sort_comparable.baml:256:40 (string) len=3 "\"c\""
// interfaces_sort_comparable.baml:257:9 (class) len=12 "SortableItem"
// interfaces_sort_comparable.baml:257:24 (property) len=4 "rank"
// interfaces_sort_comparable.baml:257:30 (number) len=1 "1"
// interfaces_sort_comparable.baml:257:33 (property) len=5 "label"
// interfaces_sort_comparable.baml:257:40 (string) len=3 "\"d\""
// interfaces_sort_comparable.baml:259:5 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:259:8 (method) len=4 "sort"
// interfaces_sort_comparable.baml:260:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:260:9 (variable) [declaration] len=6 "labels"
// interfaces_sort_comparable.baml:260:16 (operator) len=1 "="
// interfaces_sort_comparable.baml:260:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:260:21 (method) len=3 "map"
// interfaces_sort_comparable.baml:260:26 (parameter) [declaration] len=1 "x"
// interfaces_sort_comparable.baml:260:29 (class) len=12 "SortableItem"
// interfaces_sort_comparable.baml:260:46 (type) [defaultLibrary] len=6 "string"
// interfaces_sort_comparable.baml:260:55 (parameter) len=1 "x"
// interfaces_sort_comparable.baml:260:57 (property) len=5 "label"
// interfaces_sort_comparable.baml:261:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:261:9 (variable) [declaration] len=8 "expected"
// interfaces_sort_comparable.baml:261:18 (operator) len=1 "="
// interfaces_sort_comparable.baml:261:21 (string) len=3 "\"b\""
// interfaces_sort_comparable.baml:261:26 (string) len=3 "\"d\""
// interfaces_sort_comparable.baml:261:31 (string) len=3 "\"a\""
// interfaces_sort_comparable.baml:261:36 (string) len=3 "\"c\""
// interfaces_sort_comparable.baml:262:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:262:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:262:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:262:18 (variable) len=6 "labels"
// interfaces_sort_comparable.baml:262:26 (variable) len=8 "expected"
// interfaces_sort_comparable.baml:265:1 (comment) len=82 "// Fallible comparator (CompareError = ComparableCompareError): the call site must"
// interfaces_sort_comparable.baml:266:1 (comment) len=70 "// handle it, and a throw mid-sort leaves the array in pre-sort state."
// interfaces_sort_comparable.baml:267:1 (keyword) len=8 "function"
// interfaces_sort_comparable.baml:267:10 (function) [declaration] len=10 "sort_flaky"
// interfaces_sort_comparable.baml:267:21 (parameter) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:267:25 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:267:47 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:267:65 (keyword) len=6 "throws"
// interfaces_sort_comparable.baml:267:72 (class) len=22 "ComparableCompareError"
// interfaces_sort_comparable.baml:268:5 (parameter) len=2 "xs"
// interfaces_sort_comparable.baml:268:8 (method) len=4 "sort"
// interfaces_sort_comparable.baml:271:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:271:6 (string) len=43 "\"sort_fallible_comparator_throw_rolls_back\""
// interfaces_sort_comparable.baml:272:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:272:9 (variable) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:272:12 (operator) len=1 "="
// interfaces_sort_comparable.baml:273:9 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:273:27 (property) len=5 "value"
// interfaces_sort_comparable.baml:273:34 (number) len=1 "3"
// interfaces_sort_comparable.baml:274:9 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:274:27 (property) len=5 "value"
// interfaces_sort_comparable.baml:274:34 (type) [defaultLibrary] len=4 "null"
// interfaces_sort_comparable.baml:275:9 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:275:27 (property) len=5 "value"
// interfaces_sort_comparable.baml:275:34 (number) len=1 "1"
// interfaces_sort_comparable.baml:277:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:277:9 (variable) [declaration] len=3 "msg"
// interfaces_sort_comparable.baml:277:13 (operator) len=1 "="
// interfaces_sort_comparable.baml:278:9 (function) len=10 "sort_flaky"
// interfaces_sort_comparable.baml:278:20 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:279:9 (string) len=10 "\"no throw\""
// interfaces_sort_comparable.baml:280:7 (keyword) len=5 "catch"
// interfaces_sort_comparable.baml:280:14 (parameter) [declaration] len=1 "e"
// interfaces_sort_comparable.baml:281:9 (class) len=22 "ComparableCompareError"
// interfaces_sort_comparable.baml:281:35 (parameter) len=1 "e"
// interfaces_sort_comparable.baml:281:37 (property) len=7 "message"
// interfaces_sort_comparable.baml:283:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:283:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:283:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:283:20 (variable) len=3 "msg"
// interfaces_sort_comparable.baml:283:24 (method) len=8 "includes"
// interfaces_sort_comparable.baml:283:33 (string) len=21 "\"cannot compare null\""
// interfaces_sort_comparable.baml:284:5 (comment) len=34 "// rollback: original order intact"
// interfaces_sort_comparable.baml:285:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:285:9 (variable) [declaration] len=4 "vals"
// interfaces_sort_comparable.baml:285:14 (operator) len=1 "="
// interfaces_sort_comparable.baml:285:16 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:285:19 (method) len=3 "map"
// interfaces_sort_comparable.baml:285:24 (parameter) [declaration] len=1 "x"
// interfaces_sort_comparable.baml:285:27 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:285:47 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:286:9 (keyword) len=2 "if"
// interfaces_sort_comparable.baml:286:12 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:286:16 (variable) [declaration] len=1 "v"
// interfaces_sort_comparable.baml:286:19 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:286:23 (operator) len=1 "="
// interfaces_sort_comparable.baml:286:25 (parameter) len=1 "x"
// interfaces_sort_comparable.baml:286:27 (property) len=5 "value"
// interfaces_sort_comparable.baml:286:35 (variable) len=1 "v"
// interfaces_sort_comparable.baml:286:39 (keyword) len=4 "else"
// interfaces_sort_comparable.baml:286:46 (operator) len=1 "-"
// interfaces_sort_comparable.baml:286:47 (number) len=1 "1"
// interfaces_sort_comparable.baml:288:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:288:9 (variable) [declaration] len=8 "expected"
// interfaces_sort_comparable.baml:288:18 (operator) len=1 "="
// interfaces_sort_comparable.baml:288:21 (number) len=1 "3"
// interfaces_sort_comparable.baml:288:24 (operator) len=1 "-"
// interfaces_sort_comparable.baml:288:25 (number) len=1 "1"
// interfaces_sort_comparable.baml:288:28 (number) len=1 "1"
// interfaces_sort_comparable.baml:289:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:289:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:289:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:289:18 (variable) len=4 "vals"
// interfaces_sort_comparable.baml:289:24 (variable) len=8 "expected"
// interfaces_sort_comparable.baml:292:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:292:6 (string) len=37 "\"sort_fallible_comparator_happy_path\""
// interfaces_sort_comparable.baml:293:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:293:9 (variable) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:293:12 (operator) len=1 "="
// interfaces_sort_comparable.baml:294:9 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:294:27 (property) len=5 "value"
// interfaces_sort_comparable.baml:294:34 (number) len=1 "3"
// interfaces_sort_comparable.baml:295:9 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:295:27 (property) len=5 "value"
// interfaces_sort_comparable.baml:295:34 (number) len=1 "1"
// interfaces_sort_comparable.baml:296:9 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:296:27 (property) len=5 "value"
// interfaces_sort_comparable.baml:296:34 (number) len=1 "2"
// interfaces_sort_comparable.baml:298:5 (comment) len=72 "// No-throw contract: all values present, so the comparator never fails."
// interfaces_sort_comparable.baml:299:5 (comment) len=72 "// Asserted inline — a catch result stored in a test-block `let` stays"
// interfaces_sort_comparable.baml:300:5 (comment) len=70 "// boxed (same VM quirk as above) and `== false` would reject the box."
// interfaces_sort_comparable.baml:301:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:301:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:301:12 (function) len=7 "is_true"
// interfaces_sort_comparable.baml:302:9 (function) len=10 "sort_flaky"
// interfaces_sort_comparable.baml:302:20 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:303:9 (boolean) len=5 "false"
// interfaces_sort_comparable.baml:304:7 (keyword) len=5 "catch"
// interfaces_sort_comparable.baml:304:14 (parameter) [declaration] len=1 "e"
// interfaces_sort_comparable.baml:305:9 (class) len=22 "ComparableCompareError"
// interfaces_sort_comparable.baml:305:35 (boolean) len=4 "true"
// interfaces_sort_comparable.baml:306:8 (operator) len=2 "=="
// interfaces_sort_comparable.baml:306:11 (boolean) len=5 "false"
// interfaces_sort_comparable.baml:307:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:307:9 (variable) [declaration] len=4 "vals"
// interfaces_sort_comparable.baml:307:14 (operator) len=1 "="
// interfaces_sort_comparable.baml:307:16 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:307:19 (method) len=3 "map"
// interfaces_sort_comparable.baml:307:24 (parameter) [declaration] len=1 "x"
// interfaces_sort_comparable.baml:307:27 (class) len=15 "ComparableFlaky"
// interfaces_sort_comparable.baml:307:47 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:308:9 (keyword) len=2 "if"
// interfaces_sort_comparable.baml:308:12 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:308:16 (variable) [declaration] len=1 "v"
// interfaces_sort_comparable.baml:308:19 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:308:23 (operator) len=1 "="
// interfaces_sort_comparable.baml:308:25 (parameter) len=1 "x"
// interfaces_sort_comparable.baml:308:27 (property) len=5 "value"
// interfaces_sort_comparable.baml:308:35 (variable) len=1 "v"
// interfaces_sort_comparable.baml:308:39 (keyword) len=4 "else"
// interfaces_sort_comparable.baml:308:46 (operator) len=1 "-"
// interfaces_sort_comparable.baml:308:47 (number) len=1 "1"
// interfaces_sort_comparable.baml:310:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:310:9 (variable) [declaration] len=8 "expected"
// interfaces_sort_comparable.baml:310:18 (operator) len=1 "="
// interfaces_sort_comparable.baml:310:21 (number) len=1 "1"
// interfaces_sort_comparable.baml:310:24 (number) len=1 "2"
// interfaces_sort_comparable.baml:310:27 (number) len=1 "3"
// interfaces_sort_comparable.baml:311:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:311:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:311:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:311:18 (variable) len=4 "vals"
// interfaces_sort_comparable.baml:311:24 (variable) len=8 "expected"
// interfaces_sort_comparable.baml:314:1 (comment) len=71 "// Generic propagation: a function over `U extends Comparable` may call"
// interfaces_sort_comparable.baml:315:1 (comment) len=74 "// `xs.sort()`, propagating `U.CompareError` symbolically; the test routes"
// interfaces_sort_comparable.baml:316:1 (comment) len=75 "// through it to pin the runtime instantiation too. The array is built in a"
// interfaces_sort_comparable.baml:317:1 (comment) len=73 "// wrapper *function* — a test-block local passed to a generic function"
// interfaces_sort_comparable.baml:318:1 (comment) len=74 "// arrives boxed (the VM quirk above) and the native sort rejects the box."
// interfaces_sort_comparable.baml:319:1 (keyword) len=8 "function"
// interfaces_sort_comparable.baml:319:10 (function) [declaration] len=12 "sort_generic"
// interfaces_sort_comparable.baml:319:22 (operator) len=1 "<"
// interfaces_sort_comparable.baml:319:23 (typeParameter) [declaration] len=1 "U"
// interfaces_sort_comparable.baml:319:25 (keyword) len=7 "extends"
// interfaces_sort_comparable.baml:319:33 (namespace) [defaultLibrary] len=4 "baml"
// interfaces_sort_comparable.baml:319:37 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:319:38 (interface) [defaultLibrary] len=10 "Comparable"
// interfaces_sort_comparable.baml:319:48 (operator) len=1 ">"
// interfaces_sort_comparable.baml:319:50 (parameter) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:319:54 (type) len=1 "U"
// interfaces_sort_comparable.baml:319:62 (type) len=1 "U"
// interfaces_sort_comparable.baml:319:66 (keyword) len=6 "throws"
// interfaces_sort_comparable.baml:319:73 (type) len=1 "U"
// interfaces_sort_comparable.baml:319:74 (operator) len=1 "."
// interfaces_sort_comparable.baml:319:75 (type) len=12 "CompareError"
// interfaces_sort_comparable.baml:320:5 (parameter) len=2 "xs"
// interfaces_sort_comparable.baml:320:8 (method) len=4 "sort"
// interfaces_sort_comparable.baml:323:1 (keyword) len=8 "function"
// interfaces_sort_comparable.baml:323:10 (function) [declaration] len=21 "run_sort_generic_ints"
// interfaces_sort_comparable.baml:323:37 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:323:43 (keyword) len=6 "throws"
// interfaces_sort_comparable.baml:323:50 (type) [defaultLibrary] len=5 "never"
// interfaces_sort_comparable.baml:324:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:324:9 (variable) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:324:12 (operator) len=1 "="
// interfaces_sort_comparable.baml:324:15 (number) len=1 "3"
// interfaces_sort_comparable.baml:324:18 (number) len=1 "1"
// interfaces_sort_comparable.baml:324:21 (number) len=1 "2"
// interfaces_sort_comparable.baml:325:5 (function) len=12 "sort_generic"
// interfaces_sort_comparable.baml:325:18 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:328:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:328:6 (string) len=26 "\"sort_generic_propagation\""
// interfaces_sort_comparable.baml:329:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:329:9 (variable) [declaration] len=6 "result"
// interfaces_sort_comparable.baml:329:16 (operator) len=1 "="
// interfaces_sort_comparable.baml:329:18 (function) len=21 "run_sort_generic_ints"
// interfaces_sort_comparable.baml:330:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:330:9 (variable) [declaration] len=8 "expected"
// interfaces_sort_comparable.baml:330:18 (operator) len=1 "="
// interfaces_sort_comparable.baml:330:21 (number) len=1 "1"
// interfaces_sort_comparable.baml:330:24 (number) len=1 "2"
// interfaces_sort_comparable.baml:330:27 (number) len=1 "3"
// interfaces_sort_comparable.baml:331:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:331:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:331:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:331:18 (variable) len=6 "result"
// interfaces_sort_comparable.baml:331:26 (variable) len=8 "expected"
// interfaces_sort_comparable.baml:334:1 (comment) len=123 "// ─── out-of-body `implements baml.Comparable for <class>` ───────────────────"
// interfaces_sort_comparable.baml:335:1 (comment) len=2 "//"
// interfaces_sort_comparable.baml:336:1 (comment) len=73 "// The impl methods register under the synthetic `Comparable$for$<class>`"
// interfaces_sort_comparable.baml:337:1 (comment) len=74 "// name (as spelled at the impl site), in this file's namespace. Pins that"
// interfaces_sort_comparable.baml:338:1 (comment) len=75 "// `_compare_shim`'s runtime dispatch finds them for class elements — the"
// interfaces_sort_comparable.baml:339:1 (comment) len=54 "// namespaced complement to the flat-package Rust test"
// interfaces_sort_comparable.baml:340:1 (comment) len=56 "// (`sort_user_class_with_out_of_body_comparable_impl`)."
// interfaces_sort_comparable.baml:341:1 (keyword) len=5 "class"
// interfaces_sort_comparable.baml:341:7 (class) [declaration] len=14 "OutOfBodyScore"
// interfaces_sort_comparable.baml:342:5 (property) [declaration] len=6 "points"
// interfaces_sort_comparable.baml:342:12 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:345:1 (keyword) len=10 "implements"
// interfaces_sort_comparable.baml:345:12 (namespace) [defaultLibrary] len=4 "baml"
// interfaces_sort_comparable.baml:345:16 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:345:17 (interface) [defaultLibrary] len=10 "Comparable"
// interfaces_sort_comparable.baml:345:28 (keyword) len=3 "for"
// interfaces_sort_comparable.baml:345:32 (class) len=14 "OutOfBodyScore"
// interfaces_sort_comparable.baml:346:5 (keyword) len=4 "type"
// interfaces_sort_comparable.baml:346:10 (type) [declaration] len=12 "CompareError"
// interfaces_sort_comparable.baml:346:23 (operator) len=1 "="
// interfaces_sort_comparable.baml:346:25 (type) [defaultLibrary] len=5 "never"
// interfaces_sort_comparable.baml:347:5 (keyword) len=8 "function"
// interfaces_sort_comparable.baml:347:14 (method) [declaration] len=7 "compare"
// interfaces_sort_comparable.baml:347:22 (parameter) [declaration] len=4 "self"
// interfaces_sort_comparable.baml:347:28 (parameter) [declaration] len=5 "other"
// interfaces_sort_comparable.baml:347:35 (type) len=4 "Self"
// interfaces_sort_comparable.baml:347:44 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:347:48 (keyword) len=6 "throws"
// interfaces_sort_comparable.baml:347:55 (type) [defaultLibrary] len=5 "never"
// interfaces_sort_comparable.baml:348:9 (parameter) len=4 "self"
// interfaces_sort_comparable.baml:348:14 (property) len=6 "points"
// interfaces_sort_comparable.baml:348:21 (method) len=7 "compare"
// interfaces_sort_comparable.baml:348:29 (parameter) len=5 "other"
// interfaces_sort_comparable.baml:348:35 (property) len=6 "points"
// interfaces_sort_comparable.baml:352:1 (keyword) len=4 "test"
// interfaces_sort_comparable.baml:352:6 (string) len=34 "\"sort_user_class_out_of_body_impl\""
// interfaces_sort_comparable.baml:353:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:353:9 (variable) [declaration] len=2 "xs"
// interfaces_sort_comparable.baml:353:12 (operator) len=1 "="
// interfaces_sort_comparable.baml:354:9 (class) len=14 "OutOfBodyScore"
// interfaces_sort_comparable.baml:354:26 (property) len=6 "points"
// interfaces_sort_comparable.baml:354:34 (number) len=1 "3"
// interfaces_sort_comparable.baml:355:9 (class) len=14 "OutOfBodyScore"
// interfaces_sort_comparable.baml:355:26 (property) len=6 "points"
// interfaces_sort_comparable.baml:355:34 (number) len=1 "1"
// interfaces_sort_comparable.baml:356:9 (class) len=14 "OutOfBodyScore"
// interfaces_sort_comparable.baml:356:26 (property) len=6 "points"
// interfaces_sort_comparable.baml:356:34 (number) len=1 "2"
// interfaces_sort_comparable.baml:358:5 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:358:8 (method) len=4 "sort"
// interfaces_sort_comparable.baml:359:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:359:9 (variable) [declaration] len=3 "pts"
// interfaces_sort_comparable.baml:359:13 (operator) len=1 "="
// interfaces_sort_comparable.baml:359:15 (variable) len=2 "xs"
// interfaces_sort_comparable.baml:359:18 (method) len=3 "map"
// interfaces_sort_comparable.baml:359:23 (parameter) [declaration] len=1 "x"
// interfaces_sort_comparable.baml:359:26 (class) len=14 "OutOfBodyScore"
// interfaces_sort_comparable.baml:359:45 (type) [defaultLibrary] len=3 "int"
// interfaces_sort_comparable.baml:359:51 (parameter) len=1 "x"
// interfaces_sort_comparable.baml:359:53 (property) len=6 "points"
// interfaces_sort_comparable.baml:360:5 (keyword) len=3 "let"
// interfaces_sort_comparable.baml:360:9 (variable) [declaration] len=8 "expected"
// interfaces_sort_comparable.baml:360:18 (operator) len=1 "="
// interfaces_sort_comparable.baml:360:21 (number) len=1 "1"
// interfaces_sort_comparable.baml:360:24 (number) len=1 "2"
// interfaces_sort_comparable.baml:360:27 (number) len=1 "3"
// interfaces_sort_comparable.baml:361:5 (namespace) [defaultLibrary] len=6 "assert"
// interfaces_sort_comparable.baml:361:11 (namespace) [defaultLibrary] len=1 "."
// interfaces_sort_comparable.baml:361:12 (function) len=5 "equal"
// interfaces_sort_comparable.baml:361:18 (variable) len=3 "pts"
// interfaces_sort_comparable.baml:361:23 (variable) len=8 "expected"
