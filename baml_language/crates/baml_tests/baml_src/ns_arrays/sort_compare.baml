// Tests for `baml.ops.Compare` as the array-sort ordering (stdlib
// `sortable.baml`) and the `Sortable` blanket impl built on it.
//
// Builtin impls cover int/float/bigint/string. `Compare.cmp` is `throws never`,
// so sorting is infallible for every element type — there is no error channel
// on `sort()` at all. Float orders by BAML's total float order (the one `<`
// uses), so NaN has a defined position.

// NB on shape: receivers are *annotated* test-block locals and arguments are
// literals (or inline calls). Literal arguments widen into `Self` fine.
// Arguments must not be test-block locals: a local passed as the `other`
// argument of a two-`Self` interface method arrives boxed in a `test` block
// (pre-existing VM quirk, see the user-class section note below) and the
// comparison op rejects the box. Receiver-position locals unbox fine.

test "compare_int_cmp" {
    let five: int = 5;
    let three: int = 3;
    assert.is_true(five.cmp(3) == baml.ops.Ordering.Greater);
    assert.is_true(three.cmp(5) == baml.ops.Ordering.Less);
    assert.is_true(three.cmp(3) == baml.ops.Ordering.Equal)
}

test "compare_string_cmp" {
    let a: string = "a";
    let b: string = "b";
    let aa: string = "aa";
    assert.is_true(a.cmp("b") == baml.ops.Ordering.Less);
    assert.is_true(b.cmp("a") == baml.ops.Ordering.Greater);
    assert.is_true(aa.cmp("a") == baml.ops.Ordering.Greater);
    assert.is_true(a.cmp("a") == baml.ops.Ordering.Equal)
}

test "compare_bigint_cmp" {
    let one: bigint = 1n;
    let two: bigint = 2n;
    assert.is_true(two.cmp(1n) == baml.ops.Ordering.Greater);
    assert.is_true(one.cmp(2n) == baml.ops.Ordering.Less);
    assert.is_true(two.cmp(2n) == baml.ops.Ordering.Equal)
}

test "compare_float_cmp" {
    let lo: float = 1.5;
    let hi: float = 2.5;
    assert.is_true(hi.cmp(1.5) == baml.ops.Ordering.Greater);
    assert.is_true(lo.cmp(2.5) == baml.ops.Ordering.Less);
    assert.is_true(lo.cmp(1.5) == baml.ops.Ordering.Equal)
}

// Float comparison never fails on NaN: BAML's total float order puts NaN after
// +inf (`… < +inf < NaN`) and equal to itself.
test "compare_float_nan_total_order" {
    let one: float = 1.0;
    assert.is_true(one.cmp(float.nan()) == baml.ops.Ordering.Less);
    assert.is_true(float.nan().cmp(1.0) == baml.ops.Ordering.Greater);
    assert.is_true(float.nan().cmp(float.inf()) == baml.ops.Ordering.Greater);
    assert.is_true(float.nan().cmp(float.nan()) == baml.ops.Ordering.Equal)
}

// ─── user classes opting in ──────────────────────────────────────────────────
//
// KNOWN VM BUG (pre-existing, not ordering-specific): a class instance read
// back from a *test-block local* stays boxed, and passing a boxed value to a
// two-`Self`-param interface method (`cmp(self, other: Self)`) panics the VM
// with "unreachable code executed". The same calls work from ordinary
// functions. Workaround used throughout: construct instances at the call
// boundary (inline arguments), never via test-block `let`.
//
// `Compare requires Equals`, so an ordered user type implements both.

class CompareScore {
    points int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never {
            self.points == other.points
        }
    }
    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering throws never {
            self.points.cmp(other.points)
        }
    }
}

test "compare_user_class_cmp_is_infallible" {
    assert.is_true(
        CompareScore { points: 9 }.cmp(CompareScore { points: 1 })
            == baml.ops.Ordering.Greater
    );
    assert.is_true(
        CompareScore { points: 1 }.cmp(CompareScore { points: 9 })
            == baml.ops.Ordering.Less
    );
    assert.is_true(
        CompareScore { points: 1 }.cmp(CompareScore { points: 1 })
            == baml.ops.Ordering.Equal
    )
}

// ─── Sortable: `sort` ───────────────────────────────────────────────────────
//
// These exercise `sort()` on *local* array receivers (`xs.sort()`).
//
// NB: these run the sort on a *local* receiver rather than through a
// `sort_ints(xs)` helper. Passing a test-block `let` local to a function
// delivers it as `Null` inside the callee — a pre-existing VM quirk of `test`
// blocks (see the `arrays.baml` header note), unrelated to sort. Param-receiver
// dispatch itself works in real functions (Rust `phase3_*` tests). The helper
// functions below are still *declared* (and compile) to pin that `sort()` is
// `throws never` for parameter receivers at compile time.

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

function sort_floats(xs: float[]) -> float[] throws never {
    xs.sort()
}

test "sort_floats_match_sort" {
    let xs = [2.5, 0.5, 1.5];
    xs.sort();
    let expected = [0.5, 1.5, 2.5];
    assert.equal(xs, expected)
}

// NaN never throws — BAML's float order is total, so NaN sorts deterministically
// after +inf.
test "sort_floats_nan_sorts_last" {
    let xs = [1.0, float.nan(), 0.5];
    xs.sort();
    assert.equal(xs.length(), 3);
    assert.equal(xs.at(0), 0.5);
    assert.equal(xs.at(1), 1.0);
    let last = xs.at(2);
    match (last) {
        null => assert.is_true(false)
        let f: float => assert.is_true(f.is_nan())
    }
}

// A user class sorts by its `cmp` — stably, in place, returning self. The
// element type is a downstream class the stdlib cannot see, so this pins that
// `Sortable.sort`'s fallback reaches it by virtual dispatch on `T`.
class SortableItem {
    rank int
    label string
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.rank == other.rank }
    }
    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering throws never {
            self.rank.cmp(other.rank)
        }
    }
}

// Declared (and compiles) to pin that `sort()` on a user `Compare` type is
// `throws never`; the runtime tests below dispatch on local receivers (see the
// param-receiver limitation note above).
function sort_items(xs: SortableItem[]) -> SortableItem[] throws never {
    xs.sort()
}

test "sort_user_class_sorts_by_cmp" {
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

// ─── fallible ordering lives on `sort_by`, not `sort` ───────────────────────
//
// `Compare.cmp` is `throws never`, so `sort()` has no error channel. A sort
// that can fail is written with `sort_by`, whose comparator keeps its own
// `throws E` — including the roll-back-on-throw contract.

class SortCompareError {
    message string
}

class Flaky {
    value int?
}

function flaky_cmp(a: Flaky, b: Flaky) -> baml.ops.Ordering throws SortCompareError {
    if let x: int = a.value {
        if let y: int = b.value {
            return x.cmp(y)
        }
    }
    throw SortCompareError { message: "cannot compare null" }
}

function sort_flaky(xs: Flaky[]) -> Flaky[] throws SortCompareError {
    xs.sort_by(flaky_cmp)
}

test "sort_by_throwing_comparator_rolls_back" {
    let xs = [
        Flaky { value: 3 },
        Flaky { value: null },
        Flaky { value: 1 }
    ];
    let msg = {
        sort_flaky(xs);
        "no throw"
    } catch (e) {
        SortCompareError => e.message
    };
    assert.is_true(msg.includes("cannot compare null"));
    // rollback: original order intact
    let vals = xs.map((x: Flaky) -> int {
        if let v: int = x.value { v } else { -1 }
    });
    let expected = [3, -1, 1];
    assert.equal(vals, expected)
}

test "sort_by_throwing_comparator_happy_path" {
    let xs = [
        Flaky { value: 3 },
        Flaky { value: 1 },
        Flaky { value: 2 }
    ];
    // No-throw contract: all values present, so the comparator never fails.
    // Asserted inline — a catch result stored in a test-block `let` stays
    // boxed (same VM quirk as above) and `== false` would reject the box.
    assert.is_true(({
        sort_flaky(xs);
        false
    } catch (e) {
        SortCompareError => true
    }) == false);
    let vals = xs.map((x: Flaky) -> int {
        if let v: int = x.value { v } else { -1 }
    });
    let expected = [1, 2, 3];
    assert.equal(vals, expected)
}

// Generic propagation: a function over `U extends baml.ops.Compare` may call
// `xs.sort()`, and it is `throws never` at every instantiation. The test routes
// through it to pin the runtime instantiation too. The array is built in a
// wrapper *function* — a test-block local passed to a generic function arrives
// boxed (the VM quirk above) and the native sort rejects the box.
function sort_generic<U extends baml.ops.Compare>(xs: U[]) -> U[] throws never {
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

// ─── out-of-body `implements baml.ops.Compare for <class>` ──────────────────
//
// The impl methods register under the synthetic `Compare$for$<class>` name (as
// spelled at the impl site), in this file's namespace. Pins that `sort()`'s
// runtime dispatch finds them for class elements.
class OutOfBodyScore {
    points int
}

implements baml.ops.Equals for OutOfBodyScore {
    function eq(self, other: Self) -> bool throws never { self.points == other.points }
}

implements baml.ops.Compare for OutOfBodyScore {
    function cmp(self, other: Self) -> baml.ops.Ordering throws never {
        self.points.cmp(other.points)
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
