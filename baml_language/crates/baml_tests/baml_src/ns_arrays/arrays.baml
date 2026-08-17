// Unified tests for array construction and methods.
//
// Converted from crates/baml_tests/tests/arrays.rs. Each Rust test becomes a
// `test` block with the `main()` body inlined. Results are compared with
// `assert.equal`, which is structural: `==` on arrays recurses element-wise
// through `baml.ops.equals_equals`.
//
// A `catch` expression's value is passed directly into the assertion call rather
// than bound to a `let`: a stored result stays boxed and won't compare equal, but
// the call boundary coerces it back to its concrete type.
//
// Inside a `test` block, a value read from a `let` local compares unequal to a
// literal — a VM bug that does not affect the same code inside a `function`.
// Tests that must hold a value in a local (mutation results, `count`
// accumulators) therefore bind the expected value to a local too, since two
// locals compare correctly.

class ArraySortItem {
    key int
    label string
    nullable int?
}

test "array_literal" {
    assert.equal([1, 2, 3], [1, 2, 3])
}

test "array_assign_to_variable" {
    let a = [1, 2, 3];
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [1, 2, 3];
    assert.equal(a, expected)
}

test "array_push" {
    let a: int[] = [1, 2, 3];
    a.push(4);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [1, 2, 3, 4];
    assert.equal(a, expected)
}

// ─── sort ────────────────────────────────────────────────────────────────────

test "array_sort_ints_in_place" {
    let xs = [3, 1, 2];
    let result = xs.sort();
    let expected = [1, 2, 3];
    assert.is_true(result == xs);
    assert.equal(xs, expected)
}

test "array_sort_returns_self" {
    let xs = [3, 1, 2];
    let result = xs.sort();
    assert.is_true(result == xs);
    result.push(4);
    let expected = [1, 2, 3, 4];
    assert.equal(xs, expected)
}

test "array_sort_empty_is_noop" {
    let xs: int[] = [];
    let result = xs.sort();
    let expected: int[] = [];
    assert.is_true(result == xs);
    assert.equal(xs, expected)
}

test "array_sort_duplicates_stable" {
    let xs = [3, 1, 2, 1, 3];
    xs.sort();
    let expected = [1, 1, 2, 3, 3];
    assert.equal(xs, expected)
}

test "array_sort_strings" {
    let xs = ["b", "a", "aa"];
    xs.sort();
    let expected = ["a", "aa", "b"];
    assert.equal(xs, expected)
}

test "array_sort_bigints" {
    let xs = [3n, 1n, 2n];
    xs.sort();
    let expected = [1n, 2n, 3n];
    assert.equal(xs, expected)
}

test "array_sort_floats" {
    let xs = [2.5, 0.5, 1.5];
    let result = xs.sort();
    let expected = [0.5, 1.5, 2.5];
    assert.is_true(result == xs);
    assert.equal(xs, expected)
}

// PHASE-5 cutover: `sort()` is now the `Sortable` blanket method requiring
// `T implements Comparable`. The previously-runtime rejections below became
// compile errors and moved to Rust `assert_compile_error_*` tests in
// `tests/comparable_sort.rs` (`phase5_*`):
//   • `(int | float)[].sort()` — unions can't implement `Comparable`
//   • `(float | bigint)[].sort()` — likewise
//   • `int?[].sort()` (optional = union)
//   • `ArraySortItem[].sort()` (class without a `Comparable` impl)
//   • `(int | string)[].sort()` — rollback now pinned via `sort_by`
//     (`array_sort_by_callback_throws`)
// 02-PLAN cutover (reverses the 01b NaN decision): `float` now binds
// `type CompareError = never` and orders by `f64::total_cmp`, so `float[].sort()`
// no longer rejects NaN — NaN sorts deterministically after +inf.
test "array_sort_nan_sorts_last" {
    let xs = [1.0, float.nan()];
    xs.sort();
    assert.equal(xs.at(0), 1.0);
    match (xs.at(1)) {
        null => assert.is_true(false)
        let f: float => assert.is_true(f.is_nan())
    }
}

// ─── sort_by ─────────────────────────────────────────────────────────────────

test "array_sort_by_descending" {
    let xs = [1, 3, 2];
    xs.sort_by((a: int, b: int) -> int { b - a });
    let expected = [3, 2, 1];
    assert.equal(xs, expected)
}

test "array_sort_by_returns_self" {
    let xs = [1, 3, 2];
    let result = xs.sort_by((a: int, b: int) -> int { a - b });
    assert.is_true(result == xs);
    result.push(4);
    let expected = [1, 2, 3, 4];
    assert.equal(xs, expected)
}

test "array_sort_by_callback_throws" {
    let xs = [1, 3, 2];
    let caught = {
        xs.sort_by((a: int, b: int) -> int throws string {
            if (a == 2 || b == 2) { throw "bad compare" }
            a - b
        });
        false
    } catch (e) {
        string => true
    };
    let expected_caught = true;
    assert.equal(caught, expected_caught);
    let expected = [1, 3, 2];
    assert.equal(xs, expected)
}

test "array_sort_by_later_merge_throw_does_not_write_back" {
    let xs = [4, 3, 2, 1];
    let comparisons = 0;
    assert.is_true({
        xs.sort_by((a: int, b: int) -> int throws string {
            comparisons += 1;
            // The first two calls sort the width-1 runs. The third call is the
            // first comparison in the next merge pass.
            if (comparisons == 3) { throw "later merge failed" }
            a - b
        });
        false
    } catch (e) {
        string => true
    });
    let expected_comparisons = 3;
    assert.equal(comparisons, expected_comparisons);
    let expected = [4, 3, 2, 1];
    assert.equal(xs, expected)
}

test "array_sort_by_is_stable" {
    let xs = [
        ArraySortItem { key: 2, label: "a", nullable: 2 },
        ArraySortItem { key: 1, label: "b", nullable: 1 },
        ArraySortItem { key: 2, label: "c", nullable: 2 },
        ArraySortItem { key: 1, label: "d", nullable: 1 }
    ];
    xs.sort_by((a: ArraySortItem, b: ArraySortItem) -> int { a.key - b.key });
    let labels = xs.map((x: ArraySortItem) -> string { x.label });
    let expected = ["b", "d", "a", "c"];
    assert.equal(labels, expected)
}

// A fixed-size operation-count assertion is deterministic and distinguishes
// O(n log n) sorting from the old insertion sort without relying on CI timing.
// Reverse input plus an ascending comparator is the old implementation's
// quadratic case: it made n(n-1)/2 = 32,640 calls for n=256.
test "array_sort_by_comparison_count_is_n_log_n" {
    let xs: int[] = [];
    let i = 256;
    while (i > 0) {
        xs.push(i);
        i -= 1;
    }
    let comparisons = 0;
    xs.sort_by((a: int, b: int) -> int {
        comparisons += 1;
        a - b
    });
    assert.is_true(comparisons <= 4096);
    assert.equal(xs.at(0), 1);
    assert.equal(xs.at(255), 256)
}

// Bottom-up merge passes visit pairs deterministically and callbacks never run
// concurrently. The public contract does not promise this exact sequence, but
// pinning it here makes an accidental scheduler/order change explicit.
test "array_sort_by_comparator_order_is_deterministic" {
    let seen: string[] = [];
    let xs = [4, 3, 2, 1];
    xs.sort_by((a: int, b: int) -> int {
        seen.push(`${a}:${b}`);
        a - b
    });
    let expected = ["4:3", "2:1", "3:1", "3:2"];
    assert.equal(seen, expected)
}

// ─── sort_by_key ─────────────────────────────────────────────────────────────

test "array_sort_by_key_class_field" {
    let xs = [
        ArraySortItem { key: 3, label: "c", nullable: 3 },
        ArraySortItem { key: 1, label: "a", nullable: 1 },
        ArraySortItem { key: 2, label: "b", nullable: 2 }
    ];
    xs.sort_by_key((x: ArraySortItem) -> int { x.key });
    let labels = xs.map((x: ArraySortItem) -> string { x.label });
    let expected = ["a", "b", "c"];
    assert.equal(labels, expected)
}

test "array_sort_by_key_stable" {
    let xs = [
        ArraySortItem { key: 2, label: "a", nullable: 2 },
        ArraySortItem { key: 1, label: "b", nullable: 1 },
        ArraySortItem { key: 2, label: "c", nullable: 2 },
        ArraySortItem { key: 1, label: "d", nullable: 1 }
    ];
    xs.sort_by_key((x: ArraySortItem) -> int { x.key });
    let labels = xs.map((x: ArraySortItem) -> string { x.label });
    let expected = ["b", "d", "a", "c"];
    assert.equal(labels, expected)
}

test "array_sort_by_key_called_once_per_element" {
    let count = 0;
    let xs = [3, 1, 2];
    xs.sort_by_key((x: int) -> int {
        count = count + 1;
        x
    });
    let expected_count = 3;
    assert.equal(count, expected_count);
    let expected = [1, 2, 3];
    assert.equal(xs, expected)
}

test "array_sort_by_key_calls_left_to_right_once" {
    let seen: int[] = [];
    let xs = [3, 1, 2];
    xs.sort_by_key((x: int) -> int {
        seen.push(x);
        x
    });
    let expected_seen = [3, 1, 2];
    assert.equal(seen, expected_seen);
    let expected = [1, 2, 3];
    assert.equal(xs, expected)
}

test "array_sort_by_key_empty_does_not_call_callback" {
    let xs: int[] = [];
    let result = xs.sort_by_key((x: int) -> int throws string { throw "should not call" });
    let expected: int[] = [];
    assert.is_true(result == xs);
    assert.equal(xs, expected)
}

test "array_sort_by_key_callback_throws" {
    let xs = [3, 1, 2];
    let caught = {
        xs.sort_by_key((x: int) -> int throws string {
            if (x == 1) { throw "bad key" }
            x
        });
        false
    } catch (e) {
        string => true
    };
    let expected_caught = true;
    assert.equal(caught, expected_caught)
}

test "array_sort_by_key_throw_does_not_write_back" {
    let xs = [3, 1, 2];
    let caught = {
        xs.sort_by_key((x: int) -> int throws string {
            if (x == 1) { throw "bad key" }
            x
        });
        false
    } catch (e) {
        string => true
    };
    let expected_caught = true;
    assert.equal(caught, expected_caught);
    let expected = [3, 1, 2];
    assert.equal(xs, expected)
}

// `sort_by_key` now requires the key type to be `Comparable` (it sorts by the
// key's natural order, `Comparable.compare`). A nullable key (`int?` = a union)
// cannot implement `Comparable`, so it is a *compile* error rather than the old
// runtime `InvalidArgument` — pinned by the Rust test
// `phase5_sort_by_key_nullable_key_is_compile_error`.

test "array_sort_by_key_returns_self" {
    let xs = [3, 1, 2];
    let result = xs.sort_by_key((x: int) -> int { x });
    assert.is_true(result == xs);
    result.push(4);
    let expected = [1, 2, 3, 4];
    assert.equal(xs, expected)
}

// ============================================================================
// Array.map tests
// ============================================================================

// array.map with a simple doubling function (no captures).
// [1, 2, 3].map(x -> x * 2) returns [2, 4, 6].
test "array_map_simple_double" {
    let items: int[] = [1, 2, 3];
    assert.equal(items.map((x: int) -> int { x * 2 }), [2, 4, 6])
}

// array.map on an empty array returns [].
test "array_map_empty_array" {
    let items: int[] = [];
    assert.equal(items.map((x: int) -> int { x * 2 }), [])
}

// array.map with a single-element array.
// [42].map(x -> x + 1) returns [43].
test "array_map_single_element" {
    let items: int[] = [42];
    assert.equal(items.map((x: int) -> int { x + 1 }), [43])
}

// array.map with a closure capturing multiple variables.
// [1, 2, 3].map(x -> x * scale + bias) where scale=10, bias=5 returns [15, 25, 35].
test "array_map_closure_captures_multiple_variables" {
    let scale = 10;
    let bias = 5;
    let items: int[] = [1, 2, 3];
    assert.equal(items.map((x: int) -> int { x * scale + bias }), [15, 25, 35])
}

// array.map where the callback throws — verify exception propagates through
// the CPS trampoline and can be caught.
test "array_map_callback_throws" {
    let items: int[] = [1, 2, 3];
    assert.equal(
        items.map((x: int) -> int { if (x == 2) { throw "bad value" } x }) catch (e) { _ => "caught" },
        "caught",
    )
}

// array.map callback inside a catch base still resolves its own parameter,
// even though the catch handler introduces a later lexical scope.
test "array_map_callback_in_catch_base_keeps_parameter_scope" {
    let items: int[] = [1, 2, 3];
    assert.equal(
        items.map((x: int) -> int { if (x == 2) { x + 10 } else { x } }) catch (e) { _ => [0] },
        [1, 12, 3],
    )
}

// array.map over string[] — exercises heap-object paths in MapContinuation
// (gc_roots, apply_forwarding) that int[] tests don't cover.
test "array_map_string_elements" {
    let items: string[] = ["a", "b", "c"];
    assert.equal(items.map((s: string) -> string { s + "!" }), ["a!", "b!", "c!"])
}

// ============================================================================
// Array.filter tests
// ============================================================================

test "array_filter_even_numbers" {
    let items = [1, 2, 3, 4, 5, 6];
    let result = items.filter((x: int) -> bool { x % 2 == 0 });
    let expected = [2, 4, 6];
    assert.equal(result, expected)
}

test "array_filter_empty_array_does_not_call_callback" {
    let items: int[] = [];
    let result = items.filter((x: int) -> bool throws string { throw "should not call" });
    let expected: int[] = [];
    assert.equal(result, expected)
}

test "array_filter_does_not_mutate_source" {
    let items = [1, 2, 3, 4];
    let result = items.filter((x: int) -> bool { x > 2 });
    let expected_result = [3, 4];
    let expected_items = [1, 2, 3, 4];
    assert.equal(result, expected_result);
    assert.equal(items, expected_items)
}

test "array_filter_callback_throws" {
    let items = [1, 2, 3];
    assert.equal(
        items.filter((x: int) -> bool throws string { if (x == 2) { throw "bad value" } true }) catch (e) { string => "caught" },
        "caught",
    )
}

test "array_filter_string_elements" {
    let items = ["ant", "bear", "cat"];
    let result = items.filter((s: string) -> bool { s.length() == 3 });
    let expected = ["ant", "cat"];
    assert.equal(result, expected)
}

// ─── clear ────────────────────────────────────────────────────────────────────

test "array_clear_non_empty" {
    let xs = [1, 2, 3];
    xs.clear();
    assert.equal(xs.length(), 0)
}

test "array_clear_already_empty_is_noop" {
    let xs: int[] = [];
    xs.clear();
    assert.equal(xs.length(), 0)
}

test "array_clear_returns_array_after" {
    let xs = [1, 2, 3];
    xs.clear();
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected: int[] = [];
    assert.equal(xs, expected)
}

// ─── remove_at ────────────────────────────────────────────────────────────────

test "array_remove_at_returns_removed_element" {
    let xs = [10, 20, 30];
    assert.equal(xs.remove_at(1), 20)
}

test "array_remove_at_modifies_array_in_place" {
    let xs = [10, 20, 30];
    let _ = xs.remove_at(1);
    let expected = [10, 30];
    assert.equal(xs, expected)
}

test "array_remove_at_zero_removes_first_element" {
    let xs = [10, 20, 30];
    assert.equal(xs.remove_at(0), 10);
    let expected = [20, 30];
    assert.equal(xs, expected)
}

test "array_remove_at_last_index_removes_last_element" {
    let xs = [10, 20, 30];
    assert.equal(xs.remove_at(2), 30);
    let expected = [10, 20];
    assert.equal(xs, expected)
}

test "array_remove_at_out_of_bounds_returns_null_and_keeps_array" {
    let xs = [10, 20, 30];
    assert.equal(xs.remove_at(99), null);
    let expected = [10, 20, 30];
    assert.equal(xs, expected)
}

test "array_remove_at_negative_counts_from_end" {
    let xs = [10, 20, 30];
    assert.equal(xs.remove_at(-1), 30);
    let expected = [10, 20];
    assert.equal(xs, expected)
}

test "array_remove_at_past_start_returns_null_and_keeps_array" {
    let xs = [10, 20, 30];
    assert.equal(xs.remove_at(-4), null);
    let expected = [10, 20, 30];
    assert.equal(xs, expected)
}

test "array_remove_at_empty_returns_null" {
    let xs: int[] = [];
    assert.equal(xs.remove_at(0), null)
}

// ─── shift ────────────────────────────────────────────────────────────────────

test "array_shift_returns_first_element" {
    let xs = [10, 20, 30];
    assert.equal(xs.shift(), 10)
}

test "array_shift_modifies_array_in_place" {
    let xs = [10, 20, 30];
    let _ = xs.shift();
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [20, 30];
    assert.equal(xs, expected)
}

test "array_shift_empty_returns_null" {
    let xs: int[] = [];
    assert.equal(xs.shift(), null)
}

test "array_shift_repeated_drains_array" {
    let xs = [1, 2, 3];
    let _ = xs.shift();
    let _ = xs.shift();
    let _ = xs.shift();
    assert.equal(xs.length(), 0)
}

// ─── unshift ──────────────────────────────────────────────────────────────────

test "array_unshift_returns_new_length" {
    let xs = [2, 3];
    assert.equal(xs.unshift(1), 3)
}

test "array_unshift_inserts_at_front" {
    let xs = [2, 3];
    let _ = xs.unshift(1);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [1, 2, 3];
    assert.equal(xs, expected)
}

test "array_unshift_empty_array" {
    let xs: int[] = [];
    let _ = xs.unshift(42);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [42];
    assert.equal(xs, expected)
}

// ─── insert ───────────────────────────────────────────────────────────────────

test "array_insert_in_middle" {
    let xs = [1, 2, 4];
    let _ = xs.insert(3, 2);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [1, 2, 3, 4];
    assert.equal(xs, expected)
}

test "array_insert_at_zero" {
    let xs = [2, 3];
    let _ = xs.insert(1, 0);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [1, 2, 3];
    assert.equal(xs, expected)
}

test "array_insert_at_length_appends" {
    let xs = [1, 2];
    let _ = xs.insert(3, xs.length());
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [1, 2, 3];
    assert.equal(xs, expected)
}

test "array_insert_returns_new_length" {
    let xs = [1, 2, 4];
    assert.equal(xs.insert(3, 2), 4)
}

test "array_insert_empty_array_at_zero" {
    let xs: int[] = [];
    let _ = xs.insert(99, 0);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [99];
    assert.equal(xs, expected)
}

// idx -1 counts from the end → inserts before the last element.
test "array_insert_negative_counts_from_end" {
    let xs = [1, 2, 3];
    assert.equal(xs.insert(9, -1), 4);
    let expected = [1, 2, 9, 3];
    assert.equal(xs, expected)
}

test "array_insert_past_start_throws" {
    {
        let xs = [1, 2, 3];
        xs.insert(0, -4);
        baml.sys.panic("expected insert to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "array_insert_past_length_throws" {
    {
        let xs = [1, 2, 3];
        xs.insert(0, 100);
        baml.sys.panic("expected insert to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── splice ───────────────────────────────────────────────────────────────────

test "array_splice_replaces_range" {
    let xs = [0, 1, 2, 3, 4];
    xs.splice(1, 2, [9, 9, 9]);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [0, 9, 9, 9, 3, 4];
    assert.equal(xs, expected)
}

test "array_splice_pure_insertion" {
    let xs = [1, 2, 3];
    xs.splice(0, 0, [-1]);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [-1, 1, 2, 3];
    assert.equal(xs, expected)
}

test "array_splice_pure_removal" {
    let xs = [0, 1, 2, 3];
    xs.splice(1, 2, []);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [0, 3];
    assert.equal(xs, expected)
}

test "array_splice_count_clamped_to_length" {
    let xs = [0, 1, 2];
    xs.splice(1, 99, [9]);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [0, 9];
    assert.equal(xs, expected)
}

test "array_splice_at_length_appends" {
    let xs = [0, 1];
    xs.splice(2, 0, [2, 3]);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [0, 1, 2, 3];
    assert.equal(xs, expected)
}

test "array_splice_replace_with_longer" {
    let xs = [0, 1, 2];
    xs.splice(1, 1, [10, 11, 12]);
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = [0, 10, 11, 12, 2];
    assert.equal(xs, expected)
}

// start -2 counts from the end → replaces starting at index length - 2.
test "array_splice_negative_start_counts_from_end" {
    let xs = [0, 1, 2, 3, 4];
    xs.splice(-2, 1, [9, 9]);
    let expected = [0, 1, 2, 9, 9, 4];
    assert.equal(xs, expected)
}

test "array_splice_past_start_throws" {
    {
        let xs = [1, 2, 3];
        xs.splice(-4, 0, []);
        baml.sys.panic("expected splice to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "array_splice_start_past_length_throws" {
    {
        let xs = [1, 2, 3];
        xs.splice(99, 0, []);
        baml.sys.panic("expected splice to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "array_splice_negative_count_throws" {
    {
        let xs = [1, 2, 3];
        xs.splice(0, -1, []);
        baml.sys.panic("expected splice to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── some ─────────────────────────────────────────────────────────────────────

test "array_some_finds_match" {
    assert.is_true([1, 2, 3].some((x: int) -> bool { x > 2 }))
}

test "array_some_no_match" {
    assert.is_true(![1, 2, 3].some((x: int) -> bool { x > 9 }))
}

test "array_some_empty_is_false" {
    let xs: int[] = [];
    assert.is_true(!xs.some((x: int) -> bool { true }))
}

// Counts predicate invocations via a closure-captured counter. If `some`
// short-circuits, the counter stops at the first match (idx 1).
test "array_some_short_circuits" {
    let count = 0;
    let _ = [1, 5, 100, 200].some((x: int) -> bool {
        count = count + 1;
        x > 4
    });
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = 2;
    assert.equal(count, expected)
}

test "array_some_propagates_error" {
    assert.is_true([1, 2, 3].some((x: int) -> bool throws string { if (x == 2) { throw "boom" } else { false } }) catch (e) { string => true })
}

// ─── every ────────────────────────────────────────────────────────────────────

test "array_every_all_match" {
    assert.is_true([2, 4, 6].every((x: int) -> bool { x % 2 == 0 }))
}

test "array_every_one_fails" {
    assert.is_true(![2, 4, 5].every((x: int) -> bool { x % 2 == 0 }))
}

test "array_every_empty_is_true" {
    let xs: int[] = [];
    assert.is_true(xs.every((x: int) -> bool { false }))
}

// visited 1, 2, 3; 3 < 3 = false → stop
test "array_every_short_circuits_on_false" {
    let count = 0;
    let _ = [1, 2, 3, 4].every((x: int) -> bool {
        count = count + 1;
        x < 3
    });
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = 3;
    assert.equal(count, expected)
}

// ─── find / find_index ────────────────────────────────────────────────────────

test "array_find_returns_first_match" {
    assert.equal([1, 2, 3, 4].find((x: int) -> bool { x > 2 }), 3)
}

test "array_find_returns_null_when_no_match" {
    assert.equal([1, 2, 3].find((x: int) -> bool { x > 9 }), null)
}

test "array_find_empty_is_null" {
    let xs: int[] = [];
    assert.equal(xs.find((x: int) -> bool { true }), null)
}

test "array_find_index_returns_first_index" {
    assert.equal([1, 2, 3, 4].find_index((x: int) -> bool { x > 2 }), 2)
}

test "array_find_index_returns_null_when_no_match" {
    assert.equal([1, 2].find_index((x: int) -> bool { x > 9 }), null)
}

test "array_find_short_circuits" {
    let count = 0;
    let _ = [10, 20, 30, 40].find((x: int) -> bool {
        count = count + 1;
        x >= 20
    });
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = 2;
    assert.equal(count, expected)
}

// ─── find_last / find_last_index ──────────────────────────────────────────────

test "array_find_last_returns_last_match" {
    assert.equal([1, 2, 3, 4].find_last((x: int) -> bool { x > 1 }), 4)
}

test "array_find_last_returns_null_when_no_match" {
    assert.equal([1, 2, 3].find_last((x: int) -> bool { x > 9 }), null)
}

// [1, 2, 1, 2] → last index where x == 1 is 2.
test "array_find_last_index_returns_last_index" {
    assert.equal([1, 2, 1, 2].find_last_index((x: int) -> bool { x == 1 }), 2)
}

// Verify iteration direction by counting visits: 40, 30, 20 — short-circuits on 20.
test "array_find_last_iterates_back_to_front" {
    let count = 0;
    let _ = [10, 20, 30, 40].find_last((x: int) -> bool {
        count = count + 1;
        x <= 20
    });
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = 3;
    assert.equal(count, expected)
}

test "array_find_last_empty_is_null" {
    let xs: int[] = [];
    assert.equal(xs.find_last((x: int) -> bool { true }), null)
}

// ─── includes / index_of / last_index_of ──────────────────────────────────────

test "array_includes_present" {
    assert.is_true([1, 2, 3].includes(2))
}

test "array_includes_absent" {
    assert.is_true(![1, 2, 3].includes(99))
}

test "array_includes_empty" {
    let xs: int[] = [];
    assert.is_true(!xs.includes(1))
}

test "array_includes_strings" {
    assert.is_true(["a", "b", "c"].includes("b"))
}

test "array_index_of_present" {
    assert.equal([10, 20, 30].index_of(20), 1)
}

// Both 10s — index_of returns the first.
test "array_index_of_first_match" {
    assert.equal([10, 20, 10].index_of(10), 0)
}

test "array_index_of_absent" {
    assert.equal([10, 20].index_of(99), null)
}

test "array_last_index_of_returns_last_match" {
    assert.equal([10, 20, 10].last_index_of(10), 2)
}

test "array_last_index_of_absent" {
    assert.equal([10, 20].last_index_of(99), null)
}

// ─── reduce ───────────────────────────────────────────────────────────────────
//
// These compare with `==` rather than `assert.equal`. `assert.equal` takes
// `unknown`, and that expected type propagates into the call as `reduce`'s
// return type, binding the accumulator `A` to `unknown | <initial>` before the
// annotated reducer is checked against `(A, T) -> A` — which then fails.
// `==` synthesizes both operands, so `A` is inferred from the reducer.
//
// BUG: an explicitly annotated reducer should pin `A` regardless of the
// expected type at the call site; the return context should not win over it.

test "array_reduce_sum" {
    assert.is_true([1, 2, 3, 4].reduce((acc: int, x: int) -> int { acc + x }, 0) == 10)
}

test "array_reduce_with_nonzero_initial" {
    assert.is_true([1, 2, 3].reduce((acc: int, x: int) -> int { acc + x }, 100) == 106)
}

test "array_reduce_empty_returns_initial" {
    let xs: int[] = [];
    assert.is_true(xs.reduce((acc: int, x: int) -> int { acc + x }, 42) == 42)
}

// 1 * 2 * 3 * 4 = 24
test "array_reduce_visits_every_element" {
    assert.is_true([1, 2, 3, 4].reduce((acc: int, x: int) -> int { acc * x }, 1) == 24)
}

test "array_reduce_concat_strings" {
    assert.is_true(
        ["a", "b", "c"].reduce((acc: string, x: string) -> string { acc + x }, "") == "abc",
    )
}

// ─── flat_map ─────────────────────────────────────────────────────────────────

test "array_flat_map_doubles_each_element" {
    assert.equal([1, 2, 3].flat_map((x: int) -> int[] { [x, x * 10] }), [1, 10, 2, 20, 3, 30])
}

test "array_flat_map_with_empty_inner_arrays" {
    assert.equal([1, 2, 3].flat_map((x: int) -> int[] { [] }), [])
}

test "array_flat_map_empty_input" {
    let xs: int[] = [];
    assert.equal(xs.flat_map((x: int) -> int[] { [x] }), [])
}

test "array_flat_map_uneven_inner_lengths" {
    assert.equal(
        [1, 2, 3].flat_map((x: int) -> int[] { if (x == 2) { [20, 21, 22] } else { [x] } }),
        [1, 20, 21, 22, 3],
    )
}

// ─── at ────────────────────────────────────────────────────────────────────────

test "array_at_positive" {
    assert.equal([10, 20, 30].at(0), 10);
    assert.equal([10, 20, 30].at(2), 30)
}

// -1 is the last element; -3 is the first.
test "array_at_negative_counts_from_end" {
    assert.equal([10, 20, 30].at(-1), 30);
    assert.equal([10, 20, 30].at(-3), 10)
}

test "array_at_past_end_is_null" {
    assert.equal([10, 20, 30].at(3), null);
    assert.equal([10, 20, 30].at(99), null)
}

// -4 normalizes to -1, before the start → null (no panic, unlike `[]`).
test "array_at_past_start_is_null" {
    assert.equal([10, 20, 30].at(-4), null)
}

test "array_at_empty_is_null" {
    let xs: int[] = [];
    assert.equal(xs.at(0), null);
    assert.equal(xs.at(-1), null)
}

// ─── subscript [] read ─────────────────────────────────────────────────────────

// A bare `[...][i]` in a test block is swallowed by the value-boxing path and
// never throws, so the indexing is wrapped in a function (see the file header).
function array_index_read(idx: int) -> int {
    let xs = [10, 20, 30];
    xs[idx]
}

test "array_index_read_positive" {
    assert.equal(array_index_read(0), 10)
}

// -1 is the last element; -3 is the first.
test "array_index_read_negative_counts_from_end" {
    assert.equal(array_index_read(-1), 30);
    assert.equal(array_index_read(-3), 10)
}

test "array_index_read_past_end_panics" {
    {
        array_index_read(3);
        baml.sys.panic("expected index out of bounds")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

// -4 normalizes to -1, before the start → panic (unlike `at`, which is null).
test "array_index_read_past_start_panics" {
    {
        array_index_read(-4);
        baml.sys.panic("expected index out of bounds")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

// ─── subscript [] write ────────────────────────────────────────────────────────

function array_index_write(idx: int) -> int[] {
    let xs = [10, 20, 30];
    xs[idx] = 99;
    xs
}

// Writing through a negative index targets the element counted from the end.
test "array_index_write_negative_counts_from_end" {
    assert.equal(array_index_write(-1), [10, 20, 99])
}

test "array_index_write_past_end_panics" {
    {
        array_index_write(3);
        baml.sys.panic("expected index out of bounds")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

test "array_index_write_past_start_panics" {
    {
        array_index_write(-4);
        baml.sys.panic("expected index out of bounds")
    } catch (e) {
        baml.panics.IndexOutOfBounds => null
    }
}

// ─── slice ─────────────────────────────────────────────────────────────────────

test "array_slice_basic" {
    assert.equal([1, 2, 3, 4].slice(1, 3), [2, 3])
}

test "array_slice_negative_start" {
    assert.equal([1, 2, 3].slice(-2, 3), [2, 3])
}

test "array_slice_negative_end" {
    assert.equal([1, 2, 3, 4].slice(0, -1), [1, 2, 3])
}

test "array_slice_negative_both" {
    assert.equal([1, 2, 3, 4, 5].slice(-3, -1), [3, 4])
}

// An `end` that resolves at or before `start` yields an empty array.
test "array_slice_end_before_start_is_empty" {
    assert.equal([1, 2, 3].slice(2, 1), []);
    assert.equal([1, 2, 3].slice(-1, -2), [])
}

// Out-of-range bounds clamp to `[0, length]`.
test "array_slice_clamps_out_of_range" {
    assert.equal([1, 2, 3].slice(-100, 100), [1, 2, 3])
}

test "array_slice_empty_array" {
    let xs: int[] = [];
    assert.equal(xs.slice(-1, 5), [])
}

// ============================================================================
// Array.generate tests (Linear B-638)
//
// `Array.generate(length, f)` calls `f` once per index 0..length-1 and stores
// each result in its own slot. It is the alias-free counterpart to
// `Array.filled`: because `f` runs per slot, reference-typed results (inner
// arrays, maps, class instances) are independent instead of shared.
// ============================================================================

// (a) `f` is called once per index, receiving 0, 1, ..., length-1 in order.
test "array_generate_passes_index_0_to_n_minus_1" {
    let result = baml.Array.generate(4, (i: int) -> int { i });
    let expected = [0, 1, 2, 3];
    assert.equal(result, expected)
}

// The factory result can transform the index (here: squares).
test "array_generate_maps_index_through_f" {
    let result = baml.Array.generate(3, (i: int) -> int { i * i });
    let expected = [0, 1, 4];
    assert.equal(result, expected)
}

// `f` runs exactly `length` times — once per slot, no more.
test "array_generate_calls_f_once_per_index" {
    let count = 0;
    let _ = baml.Array.generate(3, (i: int) -> int {
        count = count + 1;
        i
    });
    // `expected` is a local: a test-block local compares unequal to a literal (VM bug)
    let expected = 3;
    assert.equal(count, expected)
}

// (b) Each slot is INDEPENDENT: mutating one generated inner array must not
// touch the others — the exact aliasing bug `Array.filled(n, <mutable>)` has.
// Done in a function so the subscript writes/reads avoid test-block value-boxing
// quirks (see file header).
function generate_independence_grid() -> int[][] {
    let grid = baml.Array.generate(3, (r: int) -> int[] { baml.Array.filled(2, 0) });
    grid[0].push(99);
    grid
}

test "array_generate_slots_are_independent" {
    assert.equal(generate_independence_grid(), [[0, 0, 99], [0, 0], [0, 0]])
}

// Contrast: `Array.filled` with the same mutable inner value aliases every slot,
// so the push is visible in all three rows. This pins the behavioral difference
// that motivates `generate`.
function filled_aliases_grid() -> int[][] {
    let row = baml.Array.filled(2, 0);
    let grid = baml.Array.filled(3, row);
    grid[0].push(99);
    grid
}

test "array_filled_shares_mutable_slots_unlike_generate" {
    assert.equal(filled_aliases_grid(), [[0, 0, 99], [0, 0, 99], [0, 0, 99]])
}

// (c) n == 0 yields an empty array and never calls `f`.
test "array_generate_zero_length_is_empty" {
    let result = baml.Array.generate(0, (i: int) -> int throws string { throw "should not call" });
    let expected: int[] = [];
    assert.equal(result, expected)
}

// A negative length is clamped to empty, like `Array.filled`.
test "array_generate_negative_length_is_empty" {
    let result = baml.Array.generate(-3, (i: int) -> int throws string { throw "should not call" });
    let expected: int[] = [];
    assert.equal(result, expected)
}

// An error thrown by `f` propagates to the caller and halts generation.
test "array_generate_propagates_error" {
    assert.equal(
        baml.Array.generate(3, (i: int) -> int throws string { if (i == 1) { throw "boom" } i }) catch (e) { string => "caught" },
        "caught",
    )
}
