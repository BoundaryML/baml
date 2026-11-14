// Tuple ordering, list search, and cyclic rendering.
//
// Tuples implement `baml.ops.Compare` automatically when every slot type does,
// at any arity and through nested tuples, comparing slots lexicographically
// from left to right through each slot's own `Compare.cmp`. Names carry a
// `tc_` / `Tc` prefix to stay clear of the other namespaces in this
// one-project corpus.

// ─── Operators ───────────────────────────────────────────────────────────────

function tc_lt(a: (int, string), b: (int, string)) -> bool {
    a < b
}

test "tuple_lt_decides_on_first_unequal_slot" {
    assert.is_true(tc_lt((1, "z"), (2, "a")))
    assert.is_true(tc_lt((1, "a"), (1, "b")))
    assert.is_true(!tc_lt((1, "b"), (1, "a")))
    assert.is_true(!tc_lt((1, "a"), (1, "a")))
}

function tc_ops() -> bool[] {
    let a = (1, 2.5, true);
    let b = (1, 2.5, false);
    [a > b, a >= b, a <= b, b < a, a >= a, a <= a, a > a]
}

test "tuple_ordering_operators" {
    assert.equal(tc_ops(), [true, true, false, true, true, true, false])
}

function tc_cmp(a: (string, int), b: (string, int)) -> baml.ops.Ordering {
    a.cmp(b)
}

test "tuple_cmp_method" {
    assert.is_true(tc_cmp(("a", 1), ("a", 2)) == baml.ops.Ordering.Less)
    assert.is_true(tc_cmp(("b", 1), ("a", 2)) == baml.ops.Ordering.Greater)
    assert.is_true(tc_cmp(("a", 2), ("a", 2)) == baml.ops.Ordering.Equal)
}

function tc_singleton(a: (int,), b: (int,)) -> bool {
    a < b
}

function tc_wide() -> bool {
    (1, 2, 3, 4, 5, 6, 7, 8, 9) < (1, 2, 3, 4, 5, 6, 7, 8, 10)
}

test "tuple_ordering_every_arity" {
    assert.is_true(tc_singleton((1,), (2,)))
    assert.is_true(!tc_singleton((2,), (2,)))
    assert.is_true(tc_wide())
}

// ─── Nested tuples ───────────────────────────────────────────────────────────

function tc_nested(a: ((int, string), bool), b: ((int, string), bool)) -> bool {
    a < b
}

test "nested_tuple_ordering" {
    assert.is_true(tc_nested(((1, "a"), true), ((1, "b"), false)))
    assert.is_true(tc_nested(((1, "a"), false), ((1, "a"), true)))
    assert.is_true(!tc_nested(((2, "a"), false), ((1, "z"), true)))
}

// ─── Custom slot implementations ─────────────────────────────────────────────

// Orders by `rank` descending, ignoring `label`: the tuple must consult this
// `cmp` rather than any structural order of the class.
class TcRank {
    rank: int
    label: string
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never {
            self.rank == other.rank
        }
    }
    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering throws never {
            other.rank.cmp(self.rank)
        }
    }
}

function tc_custom_lt() -> bool[] {
    let high = TcRank { rank: 9, label: "a" };
    let low = TcRank { rank: 1, label: "z" };
    let same = TcRank { rank: 9, label: "other" };
    [(high, 0) < (low, 0), (low, 0) < (high, 0), (high, 0) < (same, 1), (high, 1) < (same, 0)]
}

test "tuple_ordering_honors_custom_slot_compare" {
    assert.equal(tc_custom_lt(), [true, false, true, false])
}

function tc_nested_custom() -> baml.ops.Ordering {
    let a = (1, (TcRank { rank: 1, label: "a" }, "x"));
    let b = (1, (TcRank { rank: 2, label: "b" }, "a"));
    a.cmp(b)
}

test "nested_tuple_ordering_honors_custom_slot_compare" {
    assert.is_true(tc_nested_custom() == baml.ops.Ordering.Greater)
}

// ─── Bounds ──────────────────────────────────────────────────────────────────

function tc_max<T extends baml.ops.Compare>(a: T, b: T) -> T {
    if (a < b) { b } else { a }
}

test "tuple_satisfies_compare_bound" {
    assert.equal(tc_max((1, "b"), (1, "a")), (1, "b"))
    assert.equal(tc_max(((0,), 2), ((1,), 0)), ((1,), 0))
}

// Bounds on the slots prove the tuple's own `Compare`.
function tc_pair_lt<A extends baml.ops.Compare, B extends baml.ops.Compare>(
    a: (A, B),
    b: (A, B),
) -> bool {
    a < b
}

function tc_pair_max<A extends baml.ops.Compare, B extends baml.ops.Compare>(
    a: (A, B),
    b: (A, B),
) -> (A, B) {
    tc_max(a, b)
}

test "slot_bounds_prove_tuple_compare" {
    assert.is_true(tc_pair_lt((1, "a"), (1, "b")))
    assert.is_true(!tc_pair_lt(("b", 1.0), ("a", 2.0)))
    assert.equal(tc_pair_max((2, "a"), (1, "z")), (2, "a"))
}

function tc_sorted() -> (int, string)[] {
    [(2, "a"), (1, "z"), (1, "b")].sort()
}

test "tuples_sort_lexicographically" {
    assert.equal(tc_sorted(), [(1, "b"), (1, "z"), (2, "a")])
}

// ─── List search ─────────────────────────────────────────────────────────────

function tc_search() -> (bool, int?, int?) {
    let xs = [(1, 2), (3, 4), (1, 2)];
    (xs.includes((1, 2)), xs.index_of((1, 2)), xs.last_index_of((1, 2)))
}

test "list_search_finds_equal_tuples" {
    assert.equal(tc_search(), (true, 0, 2))
}

function tc_search_miss() -> (bool, int?) {
    let xs = [(1, 2)];
    (xs.includes((2, 1)), xs.index_of((1, 3)))
}

test "list_search_misses_unequal_tuples" {
    assert.equal(tc_search_miss(), (false, null))
}

function tc_search_nested() -> bool {
    [(1, [2, 3])].includes((1, [2, 3]))
}

test "list_search_compares_nested_contents" {
    assert.is_true(tc_search_nested())
}

// `includes` honors a slot's `Equals` override exactly as `==` does.
function tc_search_custom() -> (bool, int?) {
    let xs = [(TcRank { rank: 1, label: "a" }, 0), (TcRank { rank: 2, label: "b" }, 0)];
    let probe = (TcRank { rank: 2, label: "different" }, 0);
    (xs.includes(probe), xs.index_of(probe))
}

test "list_search_honors_slot_equals_override" {
    assert.equal(tc_search_custom(), (true, 1))
}

class TcNode {
    next: (int, TcNode[])
}

// A cycle through a tuple: equality treats a pair already being compared as
// equal, so the search terminates.
function tc_cyclic_node() -> TcNode {
    let children: TcNode[] = [];
    let node = TcNode { next: (1, children) };
    children.push(node);
    node
}

function tc_search_cyclic() -> bool {
    [tc_cyclic_node()].includes(tc_cyclic_node())
}

test "list_search_terminates_on_cycles" {
    assert.is_true(tc_search_cyclic())
}

// ─── Cyclic rendering ────────────────────────────────────────────────────────

// A value reached again while it is still being rendered renders as `…`.
function tc_render_cyclic() -> string {
    string.from(tc_cyclic_node())
}

test "string_rendering_terminates_on_cycles_through_tuples" {
    assert.equal(tc_render_cyclic(), "TcNode { next: (1, […]) }")
}

class TcShown {
    items: (int, TcShown[])
    implements baml.ToString {
        function to_string(self) -> string {
            "shown"
        }
    }
}

class TcHolder {
    shown: TcShown
    loop: (string, TcHolder[])
}

// The override-collection pass terminates on the cycle and stays aligned with
// the renderer: the override still renders in place.
function tc_render_cyclic_override() -> string {
    let holders: TcHolder[] = [];
    let holder = TcHolder { shown: TcShown { items: (0, []) }, loop: ("x", holders) };
    holders.push(holder);
    string.from((holder, holder.shown))
}

test "override_rendering_terminates_on_cycles_through_tuples" {
    assert.equal(
        tc_render_cyclic_override(),
        "(TcHolder { shown: shown, loop: (\"x\", […]) }, shown)",
    )
}

// A shared value that is not a cycle still renders in full each time.
function tc_render_shared() -> string {
    let shared = [1];
    string.from((shared, shared))
}

test "string_rendering_repeats_shared_values" {
    assert.equal(tc_render_shared(), "([1], [1])")
}
