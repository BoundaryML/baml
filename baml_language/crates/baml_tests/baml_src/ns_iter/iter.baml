// Native coverage for the associated-type `baml.iter` stdlib.

class IterLoopBox { value int }

class IterShadow {
  function iter(self) -> string {
    "shadow"
  }
}

function iter_collect() -> int[] throws unknown {
  let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([5, 6, 7]);
  it.collect()
}

test "iter_collect" {
  assert.equal(iter_collect(), [5, 6, 7])
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
  assert.equal(iter_range(), [0, 1, 2, 3, 4])
}

function iter_for_in_range() -> int[] {
  let out: int[] = [];
  for (let x in baml.iter.Range.new(1, 4)) {
    out.push(x);
  }
  out
}

test "iter_for_in_range" {
  assert.equal(iter_for_in_range(), [1, 2, 3])
}

function iter_for_in_array_iterator() -> int[] {
  let out: int[] = [];
  for (let x in baml.iter.ArrayIterator.new([2, 4, 6])) {
    out.push(x);
  }
  out
}

test "iter_for_in_array_iterator" {
  assert.equal(iter_for_in_array_iterator(), [2, 4, 6])
}

function iter_for_in_array_literal() -> int[] {
  let out: int[] = [];
  for (let x in [10, 12]) {
    out.push(x);
  }
  out
}

test "iter_for_in_array_literal" {
  assert.equal(iter_for_in_array_literal(), [10, 12])
}

function iter_for_in_array_of_classes() -> int {
  let total = 0;
  for (let x in [IterLoopBox { value: 3 }, IterLoopBox { value: 8 }]) {
    total += x.value;
  }
  total
}

test "iter_for_in_array_of_classes" {
  assert.equal(iter_for_in_array_of_classes(), 11)
}

function iter_for_in_array_of_unions() -> int {
  let xs: (int | string)[] = [1, "two", 3];
  let total = 0;
  for (let x in xs) {
    match (x) {
      let n: int => { total += n; },
      let s: string => {
        if (s == "two") {
          total += 20;
        }
      },
    }
  }
  total
}

test "iter_for_in_array_of_unions" {
  assert.equal(iter_for_in_array_of_unions(), 24)
}

function iter_array_for_each() -> int[] {
  let out: int[] = [];
  [3, 6, 9].for_each((x: int) -> void {
    out.push(x);
  });
  out
}

test "iter_array_for_each" {
  assert.equal(iter_array_for_each(), [3, 6, 9])
}

function iter_array_first_native_methods() -> bool throws unknown {
  let mapped = [1, 2, 3].map((x: int) -> int { x * 2 });
  let flat_mapped = [1, 2].flat_map((x: int) -> int[] { [x, x + 10] });
  let reduced = [1, 2, 3, 4].reduce((acc: int, x: int) -> int { acc + x }, 0);

  mapped == [2, 4, 6] &&
    flat_mapped == [1, 11, 2, 12] &&
    reduced == 10
}

test "iter_array_first_native_methods" {
  assert.is_true(iter_array_first_native_methods())
}

function iter_array_eager_filter_map_without_collect() -> int[] throws unknown {
  [1, 2, 3, 4].filter((x: int) -> bool {
    x % 2 == 0
  }).map((x: int) -> int {
    x + 10
  })
}

test "iter_array_eager_filter_map_without_collect" {
  assert.equal(iter_array_eager_filter_map_without_collect(), [12, 14])
}

function iter_array_iter_lazy_over_source_details() -> int[] throws unknown {
  let xs = [1, 2];
  let seen = 0;
  let it = xs.iter().filter((x: int) -> bool {
    seen += 1;
    x % 2 == 1
  });
  xs.push(3);
  let out = it.collect();
  let details = [seen, out.length()];
  for (let x in out) {
    details.push(x);
  }
  details
}

test "iter_array_iter_lazy_over_source" {
  assert.equal(iter_array_iter_lazy_over_source_details(), [3, 2, 1, 3])
}

function iter_for_loop_sees_push_during_iteration() -> int[] throws unknown {
  let xs = [1, 2];
  let visited: int[] = [];
  for (let x in xs) {
    visited.push(x);
    if (x == 1) {
      xs.push(3);
    }
  }
  visited
}

test "iter_for_loop_sees_push_during_iteration" {
  assert.equal(iter_for_loop_sees_push_during_iteration(), [1, 2, 3])
}

function iter_for_loop_pop_during_iteration() -> int[] throws unknown {
  let xs = [1, 2, 3, 4];
  let visited: int[] = [];
  for (let x in xs) {
    visited.push(x);
    xs.pop();
  }
  visited
}

test "iter_for_loop_pop_during_iteration" {
  assert.equal(iter_for_loop_pop_during_iteration(), [1, 2])
}

function iter_array_eager_filter_map() -> int[] throws unknown {
  [1, 2, 3, 4].filter_map((x: int) -> int? {
    if (x % 2 == 0) {
      x * 10
    } else {
      null
    }
  })
}

test "iter_array_eager_filter_map" {
  assert.equal(iter_array_eager_filter_map(), [20, 40])
}

function iter_array_nullable_elements_details() -> int[] throws unknown {
  let xs: int?[] = [1, null, 3];
  let visited = 0;
  let nulls = 0;
  let sum = 0;
  for (let x in xs) {
    visited += 1;
    match (x) {
      null => {
        nulls += 1;
      },
      let v: int => {
        sum += v;
      },
    }
  }
  [visited, nulls, sum, xs.iter().count()]
}

test "iter_array_nullable_elements" {
  assert.equal(iter_array_nullable_elements_details(), [3, 1, 4, 3])
}

function iter_array_lazy_filter_map_collect() -> int[] throws unknown {
  [1, 2, 3, 4].iter().filter((x: int) -> bool {
    x % 2 == 0
  }).map((x: int) -> int {
    x + 10
  }).collect()
}

test "iter_array_lazy_filter_map_collect" {
  assert.equal(iter_array_lazy_filter_map_collect(), [12, 14])
}

function iter_array_first_adapter_defaults() -> bool throws unknown {
  let mapped = baml.iter.ArrayIterator.new([1, 2, 3, 4]).filter((x: int) -> bool {
    x % 2 == 0
  }).map((x: int) -> int {
    x * 10
  }).collect();

  let flattened = baml.iter.ArrayIterator.new([1, 2, 3]).filter((x: int) -> bool {
    x < 3
  }).flat_map((x: int) -> baml.iter.Iterable<Item = int, Error = never> {
    [x, x + 10]
  }).collect();

  let reduced = baml.iter.ArrayIterator.new([1, 2, 3, 4]).filter((x: int) -> bool {
    x >= 2
  }).reduce((acc: int, x: int) -> int {
    acc + x
  }, 0);

  let visited: int[] = [];
  baml.iter.ArrayIterator.new([1, 2, 3]).filter((x: int) -> bool {
    x != 2
  }).for_each((x: int) -> void {
    visited.push(x * 3);
  });

  mapped == [20, 40] &&
    flattened == [1, 11, 2, 12] &&
    reduced == 9 &&
    visited == [3, 9]
}

test "iter_array_first_adapter_defaults" {
  assert.is_true(iter_array_first_adapter_defaults())
}

function iter_array_collect_count() -> bool {
  let xs = [4, 5, 6];
  xs.iter().collect() == [4, 5, 6] && xs.iter().count() == 3
}

test "iter_array_collect_count" {
  assert.is_true(iter_array_collect_count())
}

function iter_array_filter_step_by() -> int[] throws unknown {
  baml.iter.ArrayIterator.new([1, 2, 3, 4, 5, 6]).filter((x: int) -> bool {
    x % 2 == 0
  }).step_by(2).collect()
}

test "iter_array_filter_step_by" {
  assert.equal(iter_array_filter_step_by(), [2, 6])
}

function iter_array_filter_map_fluent() -> int[] throws unknown {
  [1, 2, 3, 4].iter().filter_map((x: int) -> int? {
    if (x % 2 == 0) {
      x * 10
    } else {
      null
    }
  }).collect()
}

test "iter_array_filter_map_fluent" {
  assert.equal(iter_array_filter_map_fluent(), [20, 40])
}

function iter_array_chain_fluent() -> int[] throws unknown {
  [1, 2].iter().chain([3, 4]).collect()
}

test "iter_array_chain_fluent" {
  assert.equal(iter_array_chain_fluent(), [1, 2, 3, 4])
}

function iter_array_peekable_fluent() -> int[] throws unknown {
  let p = [8, 9].iter().peekable();
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
  out
}

test "iter_array_peekable_fluent" {
  assert.equal(iter_array_peekable_fluent(), [8, 8, 9])
}

function iter_array_as_iterable_param_inner(xs: baml.iter.Iterable<Item = int, Error = never>) -> int[] throws never {
  xs.iter().collect()
}

function iter_array_as_iterable_param() -> int[] throws never {
  iter_array_as_iterable_param_inner([10, 20, 30])
}

test "iter_array_as_iterable_param" {
  assert.equal(iter_array_as_iterable_param(), [10, 20, 30])
}

function iter_shadowed_param_method(source: baml.iter.Iterable<Item = int, Error = never>) -> string {
  let source = IterShadow {};
  source.iter()
}

test "iter_shadowed_param_method" {
  assert.equal(iter_shadowed_param_method([1, 2, 3]), "shadow")
}

function iter_shadowed_param_for_in(source: baml.iter.Iterable<Item = int, Error = never>) -> string[] {
  let source = ["a", "b"];
  let out: string[] = [];
  for (let x in source) {
    out.push(x);
  }
  out
}

test "iter_shadowed_param_for_in" {
  assert.equal(iter_shadowed_param_for_in([1, 2, 3]), ["a", "b"])
}

function iter_for_in_repeat() -> int[] {
  let out: int[] = [];
  for (let x in baml.iter.Repeat<int> { value: 7, count: 2 }) {
    out.push(x);
  }
  out
}

test "iter_for_in_repeat" {
  assert.equal(iter_for_in_repeat(), [7, 7])
}

function iter_chain() -> int[] throws unknown {
  let a: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2]);
  let b: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([3, 4]);
  a.chain(b).collect()
}

test "iter_chain" {
  assert.equal(iter_chain(), [1, 2, 3, 4])
}

function iter_repeat() -> int[] throws unknown {
  baml.iter.Repeat<int> { value: 7, count: 3 }.collect()
}

test "iter_repeat" {
  assert.equal(iter_repeat(), [7, 7, 7])
}

function iter_map() -> int[] throws unknown {
  let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2, 3]);
  it.map((x: int) -> int { x * 2 }).collect()
}

test "iter_map" {
  assert.equal(iter_map(), [2, 4, 6])
}

function iter_map_implicit_source() -> int[] throws unknown {
  baml.iter.ArrayIterator.new([1, 2, 3]).map((x: int) -> int { x * 2 }).collect()
}

test "iter_map_implicit_source" {
  assert.equal(iter_map_implicit_source(), [2, 4, 6])
}

function iter_explicit_map_coerces_to_iterator() -> int[] throws unknown {
  let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2, 3]);
  let mapped: baml.iter.Iterator<Item = int, Error = never> = baml.iter.Map<int, int, never, never> {
    source: it,
    func: (x: int) -> int { x * 3 },
  };
  mapped.collect()
}

test "iter_explicit_map_coerces_to_iterator" {
  assert.equal(iter_explicit_map_coerces_to_iterator(), [3, 6, 9])
}

function iter_filter() -> int[] throws unknown {
  let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2, 3, 4]);
  it.filter((x: int) -> bool { x % 2 == 0 }).collect()
}

test "iter_filter" {
  assert.equal(iter_filter(), [2, 4])
}

function iter_filter_implicit_source() -> int[] throws unknown {
  baml.iter.ArrayIterator.new([1, 2, 3, 4]).filter((x: int) -> bool { x % 2 == 0 }).collect()
}

test "iter_filter_implicit_source" {
  assert.equal(iter_filter_implicit_source(), [2, 4])
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
  assert.equal(iter_filter_map(), [20, 40])
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
  assert.equal(iter_filter_map_implicit_source(), [20, 40])
}

function iter_flat_map() -> int[] throws unknown {
  let it: baml.iter.Iterator<Item = int, Error = never> = baml.iter.ArrayIterator.new([1, 2]);
  it.flat_map((x: int) -> baml.iter.Iterable<Item = int, Error = never> {
    baml.iter.ArrayIterator.new([x, x + 10])
  }).collect()
}

test "iter_flat_map" {
  assert.equal(iter_flat_map(), [1, 11, 2, 12])
}

function iter_flat_map_implicit_source() -> int[] throws unknown {
  baml.iter.ArrayIterator.new([1, 2]).flat_map((x: int) -> baml.iter.Iterable<Item = int, Error = never> {
    baml.iter.ArrayIterator.new([x, x + 10])
  }).collect()
}

test "iter_flat_map_implicit_source" {
  assert.equal(iter_flat_map_implicit_source(), [1, 11, 2, 12])
}

function iter_flatten() -> int[] throws unknown {
  // `flatten`'s element type parameter (`I extends Iterable<…>`) must be a
  // concrete implementor, so the elements are typed `ArrayIterator<int>` rather
  // than the abstract `Iterable<…>` existential.
  let xs: baml.iter.ArrayIterator<int>[] = [
    baml.iter.ArrayIterator.new([1, 2]),
    baml.iter.ArrayIterator.new([3, 4]),
  ];
  let outer: baml.iter.Iterator<Item = baml.iter.ArrayIterator<int>, Error = never> = baml.iter.ArrayIterator.new(xs);
  baml.iter.flatten(outer).collect()
}

test "iter_flatten" {
  assert.equal(iter_flatten(), [1, 2, 3, 4])
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
  assert.equal(iter_peekable(), [4, 4, 5, 99])
}

function iter_step_by() -> int[] throws unknown {
  baml.iter.Range.new(0, 6).step_by(2).collect()
}

test "iter_step_by" {
  assert.equal(iter_step_by(), [0, 2, 4])
}

function iter_array_iter() -> int[] throws unknown {
  [1, 2, 3].iter().collect()
}

test "iter_array_iter" {
  assert.equal(iter_array_iter(), [1, 2, 3])
}
