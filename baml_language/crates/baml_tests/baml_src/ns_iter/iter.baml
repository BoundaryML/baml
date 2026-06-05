// Native coverage for the associated-type `baml.iter` stdlib.

function iter_collect() -> int[] throws unknown {
  let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([5, 6, 7]);
  it.collect()
}

test "iter_collect" {
  assert.is_true(baml.deep_equals(iter_collect(), [5, 6, 7]))
}

function iter_reduce() -> int throws unknown {
  let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2, 3, 4]);
  it.reduce((a: int, x: int) -> int { a + x }, 0)
}

test "iter_reduce" {
  assert.equal(iter_reduce(), 10)
}

function iter_count() -> int throws unknown {
  let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([9, 9, 9]);
  it.count()
}

test "iter_count" {
  assert.equal(iter_count(), 3)
}

function iter_some_every_find() -> bool throws unknown {
  let some_large = baml.iter.ArrayIterator.new([1, 2, 3]).some((x: int) -> bool { x > 2 });
  let every_small = baml.iter.ArrayIterator.new([1, 2, 3]).every((x: int) -> bool { x < 4 });
  let found = baml.iter.ArrayIterator.new([1, 2, 3, 4]).find((x: int) -> bool { x % 2 == 0 });
  some_large && every_small && found == 2
}

test "iter_some_every_find" {
  assert.is_true(iter_some_every_find())
}

function iter_range() -> int[] throws unknown {
  baml.iter.Range.new(0, 5).collect()
}

test "iter_range" {
  assert.is_true(baml.deep_equals(iter_range(), [0, 1, 2, 3, 4]))
}

function iter_chain() -> int[] throws unknown {
  let a: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2]);
  let b: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([3, 4]);
  a.chain(b).collect()
}

test "iter_chain" {
  assert.is_true(baml.deep_equals(iter_chain(), [1, 2, 3, 4]))
}

function iter_repeat() -> int[] throws unknown {
  baml.iter.Repeat<int> { value: 7, count: 3 }.collect()
}

test "iter_repeat" {
  assert.is_true(baml.deep_equals(iter_repeat(), [7, 7, 7]))
}

function iter_map() -> int[] throws unknown {
  let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2, 3]);
  it.map((x: int) -> int { x * 2 }).collect()
}

test "iter_map" {
  assert.is_true(baml.deep_equals(iter_map(), [2, 4, 6]))
}

function iter_map_implicit_source() -> int[] throws unknown {
  baml.iter.ArrayIterator.new([1, 2, 3]).map((x: int) -> int { x * 2 }).collect()
}

test "iter_map_implicit_source" {
  assert.is_true(baml.deep_equals(iter_map_implicit_source(), [2, 4, 6]))
}

function iter_explicit_map_coerces_to_iterator() -> int[] throws unknown {
  let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2, 3]);
  let mapped: baml.iter.Iterator<Item = int, Error = never> = baml.iter.Map<int, int, never, never> {
    iter: it,
    func: (x: int) -> int { x * 3 },
  };
  mapped.collect()
}

test "iter_explicit_map_coerces_to_iterator" {
  assert.is_true(baml.deep_equals(iter_explicit_map_coerces_to_iterator(), [3, 6, 9]))
}

function iter_filter() -> int[] throws unknown {
  let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2, 3, 4]);
  it.filter((x: int) -> bool { x % 2 == 0 }).collect()
}

test "iter_filter" {
  assert.is_true(baml.deep_equals(iter_filter(), [2, 4]))
}

function iter_filter_implicit_source() -> int[] throws unknown {
  baml.iter.ArrayIterator.new([1, 2, 3, 4]).filter((x: int) -> bool { x % 2 == 0 }).collect()
}

test "iter_filter_implicit_source" {
  assert.is_true(baml.deep_equals(iter_filter_implicit_source(), [2, 4]))
}

function iter_filter_map() -> int[] throws unknown {
  let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2, 3, 4]);
  it.filter_map((x: int) -> int? {
    if (x % 2 == 0) {
      x * 10
    } else {
      null
    }
  }).collect()
}

test "iter_filter_map" {
  assert.is_true(baml.deep_equals(iter_filter_map(), [20, 40]))
}

function iter_filter_map_implicit_source() -> int[] throws unknown {
  baml.iter.ArrayIterator.new([1, 2, 3, 4]).filter_map((x: int) -> int? {
    if (x % 2 == 0) {
      x * 10
    } else {
      null
    }
  }).collect()
}

test "iter_filter_map_implicit_source" {
  assert.is_true(baml.deep_equals(iter_filter_map_implicit_source(), [20, 40]))
}

function iter_flat_map() -> int[] throws unknown {
  let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2]);
  it.flat_map((x: int) -> baml.iter.Iterable<Item = int, Error = never> {
    baml.iter.ArrayIterator.new([x, x + 10])
  }).collect()
}

test "iter_flat_map" {
  assert.is_true(baml.deep_equals(iter_flat_map(), [1, 11, 2, 12]))
}

function iter_flat_map_implicit_source() -> int[] throws unknown {
  baml.iter.ArrayIterator.new([1, 2]).flat_map((x: int) -> baml.iter.Iterable<Item = int, Error = never> {
    baml.iter.ArrayIterator.new([x, x + 10])
  }).collect()
}

test "iter_flat_map_implicit_source" {
  assert.is_true(baml.deep_equals(iter_flat_map_implicit_source(), [1, 11, 2, 12]))
}

function iter_flatten() -> int[] throws unknown {
  let xs: baml.iter.Iterable<Item = int, Error = never>[] = [
    baml.iter.ArrayIterator.new([1, 2]),
    baml.iter.ArrayIterator.new([3, 4]),
  ];
  let outer: baml.iter.Iterator<Item = baml.iter.Iterable<Item = int, Error = never>, Error = never> = baml.iter.ArrayIterator.new(xs);
  baml.iter.flatten(outer).collect()
}

test "iter_flatten" {
  assert.is_true(baml.deep_equals(iter_flatten(), [1, 2, 3, 4]))
}

function iter_peekable() -> int[] throws unknown {
  let p = baml.iter.ArrayIterator.new([4, 5]).peekable();
  let out: int[] = [];
  match (p.peek()) {
    baml.iter.Done => {},
    let x: int => out.push(x),
  }
  match (p.next()) {
    baml.iter.Done => {},
    let x: int => out.push(x),
  }
  match (p.next()) {
    baml.iter.Done => {},
    let x: int => out.push(x),
  }
  match (p.peek()) {
    baml.iter.Done => out.push(99),
    let x: int => out.push(x),
  }
  out
}

test "iter_peekable" {
  assert.is_true(baml.deep_equals(iter_peekable(), [4, 4, 5, 99]))
}

function iter_step_by() -> int[] throws unknown {
  baml.iter.Range.new(0, 6).step_by(2).collect()
}

test "iter_step_by" {
  assert.is_true(baml.deep_equals(iter_step_by(), [0, 2, 4]))
}

function iter_array_iter() -> int[] throws unknown {
  [1, 2, 3].as<baml.iter.Iterable<Item = int, Error = never>>.iter().collect()
}

test "iter_array_iter" {
  assert.is_true(baml.deep_equals(iter_array_iter(), [1, 2, 3]))
}
