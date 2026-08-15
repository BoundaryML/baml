// Native coverage for the lazy-iterator protocol shape from the generic iterator implementation
// fixture. This keeps the executable coverage in BAML, without wiring the
// experimental `baml.iter` stdlib namespace into every test project yet.

function iter_collect() -> int[] {
  let it: user.iter_impl_generics_only.core.Iterator<int, never> = user.iter_impl_generics_only.core.ArrayIterator.new([5, 6, 7]);
  it.collect()
}

test "iter_collect" {
  assert.equal(iter_collect(), [5, 6, 7])
}

function iter_reduce() -> int {
  let it: user.iter_impl_generics_only.core.Iterator<int, never> = user.iter_impl_generics_only.core.ArrayIterator.new([1, 2, 3, 4]);
  it.reduce((a: int, x: int) -> int { a + x }, 0)
}

test "iter_reduce" {
  assert.equal(iter_reduce(), 10)
}

function iter_count() -> int {
  let it: user.iter_impl_generics_only.core.Iterator<int, never> = user.iter_impl_generics_only.core.ArrayIterator.new([9, 9, 9]);
  it.count()
}

test "iter_count" {
  assert.equal(iter_count(), 3)
}

function iter_some_every_find() -> bool {
  let some_large = user.iter_impl_generics_only.core.ArrayIterator.new([1, 2, 3]).some((x: int) -> bool { x > 2 });
  let every_small = user.iter_impl_generics_only.core.ArrayIterator.new([1, 2, 3]).every((x: int) -> bool { x < 4 });
  let found = user.iter_impl_generics_only.core.ArrayIterator.new([1, 2, 3, 4]).find((x: int) -> bool { x % 2 == 0 });
  some_large && every_small && found == 2
}

test "iter_some_every_find" {
  assert.is_true(iter_some_every_find())
}

function iter_range() -> int[] {
  user.iter_impl_generics_only.core.Range.new(0, 5).collect()
}

test "iter_range" {
  assert.equal(iter_range(), [0, 1, 2, 3, 4])
}

function iter_chain() -> int[] {
  let a: user.iter_impl_generics_only.core.Iterator<int, never> = user.iter_impl_generics_only.core.ArrayIterator.new([1, 2]);
  let b: user.iter_impl_generics_only.core.Iterator<int, never> = user.iter_impl_generics_only.core.ArrayIterator.new([3, 4]);
  a.chain(b).collect()
}

test "iter_chain" {
  assert.equal(iter_chain(), [1, 2, 3, 4])
}

function iter_repeat() -> int[] {
  user.iter_impl_generics_only.core.Repeat<int> { value: 7, count: 3 }.collect()
}

test "iter_repeat" {
  assert.equal(iter_repeat(), [7, 7, 7])
}

function iter_map() -> int[] {
  let it: user.iter_impl_generics_only.core.Iterator<int, never> = user.iter_impl_generics_only.core.ArrayIterator.new([1, 2, 3]);
  it.map((x: int) -> int { x * 2 }).collect()
}

test "iter_map" {
  assert.equal(iter_map(), [2, 4, 6])
}

function iter_map_implicit_source() -> int[] {
  user.iter_impl_generics_only.core.ArrayIterator.new([1, 2, 3]).map((x: int) -> int { x * 2 }).collect()
}

test "iter_map_implicit_source" {
  assert.equal(iter_map_implicit_source(), [2, 4, 6])
}

function iter_explicit_map_coerces_to_iterator() -> int[] {
  let it: user.iter_impl_generics_only.core.Iterator<int, never> = user.iter_impl_generics_only.core.ArrayIterator.new([1, 2, 3]);
  let mapped: user.iter_impl_generics_only.core.Iterator<int, never> = user.iter_impl_generics_only.core.Map<int, int, never, never> {
    iter: it,
    func: (x: int) -> int { x * 3 },
  };
  mapped.collect()
}

test "iter_explicit_map_coerces_to_iterator" {
  assert.equal(iter_explicit_map_coerces_to_iterator(), [3, 6, 9])
}

function iter_filter() -> int[] {
  let it: user.iter_impl_generics_only.core.Iterator<int, never> = user.iter_impl_generics_only.core.ArrayIterator.new([1, 2, 3, 4]);
  it.filter((x: int) -> bool { x % 2 == 0 }).collect()
}

test "iter_filter" {
  assert.equal(iter_filter(), [2, 4])
}

function iter_filter_implicit_source() -> int[] {
  user.iter_impl_generics_only.core.ArrayIterator.new([1, 2, 3, 4]).filter((x: int) -> bool { x % 2 == 0 }).collect()
}

test "iter_filter_implicit_source" {
  assert.equal(iter_filter_implicit_source(), [2, 4])
}

function iter_filter_map() -> int[] {
  let it: user.iter_impl_generics_only.core.Iterator<int, never> = user.iter_impl_generics_only.core.ArrayIterator.new([1, 2, 3, 4]);
  it.filter_map((x: int) -> int? {
    if (x % 2 == 0) {
      x * 10
    } else {
      null
    }
  }).collect()
}

test "iter_filter_map" {
  assert.equal(iter_filter_map(), [20, 40])
}

function iter_filter_map_implicit_source() -> int[] {
  user.iter_impl_generics_only.core.ArrayIterator.new([1, 2, 3, 4]).filter_map((x: int) -> int? {
    if (x % 2 == 0) {
      x * 10
    } else {
      null
    }
  }).collect()
}

test "iter_filter_map_implicit_source" {
  assert.equal(iter_filter_map_implicit_source(), [20, 40])
}

function iter_flat_map() -> int[] {
  let it: user.iter_impl_generics_only.core.Iterator<int, never> = user.iter_impl_generics_only.core.ArrayIterator.new([1, 2]);
  it.flat_map((x: int) -> user.iter_impl_generics_only.core.Iterable<int, never> {
    user.iter_impl_generics_only.core.ArrayIterator.new([x, x + 10])
  }).collect()
}

test "iter_flat_map" {
  assert.equal(iter_flat_map(), [1, 11, 2, 12])
}

function iter_flat_map_implicit_source() -> int[] {
  user.iter_impl_generics_only.core.ArrayIterator.new([1, 2]).flat_map((x: int) -> user.iter_impl_generics_only.core.Iterable<int, never> {
    user.iter_impl_generics_only.core.ArrayIterator.new([x, x + 10])
  }).collect()
}

test "iter_flat_map_implicit_source" {
  assert.equal(iter_flat_map_implicit_source(), [1, 11, 2, 12])
}

function iter_flatten() -> int[] {
  let xs: user.iter_impl_generics_only.core.Iterable<int, never>[] = [
    user.iter_impl_generics_only.core.ArrayIterator.new([1, 2]),
    user.iter_impl_generics_only.core.ArrayIterator.new([3, 4]),
  ];
  let outer: user.iter_impl_generics_only.core.Iterator<user.iter_impl_generics_only.core.Iterable<int, never>, never> = user.iter_impl_generics_only.core.ArrayIterator.new(xs);
  user.iter_impl_generics_only.core.flatten(outer).collect()
}

test "iter_flatten" {
  assert.equal(iter_flatten(), [1, 2, 3, 4])
}

function iter_peekable() -> int[] {
  let p = user.iter_impl_generics_only.core.ArrayIterator.new([4, 5]).peekable();
  let out: int[] = [];
  match (p.peek()) {
    user.iter_impl_generics_only.core.Done => {},
    let x: int => out.push(x),
  }
  match (p.next()) {
    user.iter_impl_generics_only.core.Done => {},
    let x: int => out.push(x),
  }
  match (p.next()) {
    user.iter_impl_generics_only.core.Done => {},
    let x: int => out.push(x),
  }
  match (p.peek()) {
    user.iter_impl_generics_only.core.Done => out.push(99),
    let x: int => out.push(x),
  }
  out
}

test "iter_peekable" {
  assert.equal(iter_peekable(), [4, 4, 5, 99])
}

function iter_step_by() -> int[] {
  user.iter_impl_generics_only.core.Range.new(0, 6).step_by(2).collect()
}

test "iter_step_by" {
  assert.equal(iter_step_by(), [0, 2, 4])
}

function iter_array_iter() -> int[] {
  user.iter_impl_generics_only.core.ArrayIterator.new([1, 2, 3]).collect()
}

test "iter_array_iter" {
  assert.equal(iter_array_iter(), [1, 2, 3])
}

function out_of_body_inferred_ctor_dispatch() -> int {
  let n: int = 11;
  let value: user.iter_impl_generics_only.core.OutOfBodyValue<int> = user.iter_impl_generics_only.core.OutOfBodyBox.new(n);
  value.get()
}

test "out_of_body_inferred_ctor_dispatch" {
  assert.equal(out_of_body_inferred_ctor_dispatch(), 11)
}
