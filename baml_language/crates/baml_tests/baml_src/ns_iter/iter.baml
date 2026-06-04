// Status snapshot of the `baml.iter` lazy-iterator protocol.
//
// WORKING (these tests pass) — base sources, consumers, and `chain`:
function iter_collect() -> int[] throws unknown {
  let it: baml.iter.Iterator<int, never> = baml.iter.ArrayIterator.new([5, 6, 7]);
  it.collect()
}
test "collect" { assert.is_true(baml.deep_equals(iter_collect(), [5, 6, 7])) }

function iter_reduce() -> int throws unknown {
  let it: baml.iter.Iterator<int, never> = baml.iter.ArrayIterator.new([1, 2, 3, 4]);
  it.reduce((a: int, x: int) -> int { a + x }, 0)
}
test "reduce" { assert.equal(iter_reduce(), 10) }

function iter_count() -> int throws unknown {
  let it: baml.iter.Iterator<int, never> = baml.iter.ArrayIterator.new([9, 9, 9]);
  it.count()
}
test "count" { assert.equal(iter_count(), 3) }

function iter_range() -> int[] throws unknown { baml.iter.Range.new(0, 5).collect() }
test "range" { assert.is_true(baml.deep_equals(iter_range(), [0, 1, 2, 3, 4])) }

function iter_chain() -> int[] throws unknown {
  let a: baml.iter.Iterator<int, never> = baml.iter.ArrayIterator.new([1, 2]);
  let b: baml.iter.Iterator<int, never> = baml.iter.ArrayIterator.new([3, 4]);
  a.chain(b).collect()
}
test "chain" { assert.is_true(baml.deep_equals(iter_chain(), [1, 2, 3, 4])) }

// ---------------------------------------------------------------------------
// KNOWN-BLOCKED (commented out) — each waits on a distinct compiler bug:
//
//   .map() / .filter() / .filter_map() / .flat_map()
//     Adapter whose interface instantiation has a UNION error arg —
//     `Iterator<R, E | E2>` — does not subtype-coerce (even an explicit
//     `Map<int,int,never,never>{}` is rejected as `Iterator<int,never>`).
//   function m() -> int[] throws unknown {
//     let it: baml.iter.Iterator<int, never> = baml.iter.ArrayIterator.new([1,2,3]);
//     it.map((x: int) -> int { x * 2 }).collect()
//   }
//   test "map" { assert.is_true(baml.deep_equals(m(), [2,4,6])) }
//
//   .step_by()  — its next() uses a C-style `for` loop (separately buggy).
//
//   [1,2,3].iter()  — the `T[]` blanket `implements` does not attach `.iter()`
//     to array values ("type int[] has no member iter").
// ---------------------------------------------------------------------------
