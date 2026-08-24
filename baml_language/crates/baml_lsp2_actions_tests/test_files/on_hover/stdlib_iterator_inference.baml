class LocalError {
  message string
}

function implicit_pipeline<[CURSOR]() -> int[] {
  let source<[CURSOR] = baml.iter.ArrayIterator.new([1, 2, 3, 4])
  let nextMethod<[CURSOR] = source.next
  let mapMethod<[CURSOR] = source.map
  let iterMethod<[CURSOR] = source.iter

  let mapped<[CURSOR] = source.map((x: int) -> int { x * 2 })
  let filtered<[CURSOR] = mapped.filter((x: int) -> bool { x > 2 })
  let filterMapped<[CURSOR] = filtered.filter_map((x: int) -> string? {
    if (x % 2 == 0) {
      "even"
    } else {
      null
    }
  })

  let stringIter<[CURSOR] = filterMapped.map((s: string) -> int { s.length() })
  let stepped<[CURSOR] = stringIter.step_by(2)
  let collected<[CURSOR] = stepped.collect()
  collected
}

function implicit_array_iter<[CURSOR]() -> int[] {
  let arrayIter<[CURSOR] = [1, 2, 3].iter()
  let found<[CURSOR] = arrayIter.find((x: int) -> bool { x == 2 })
  let collected<[CURSOR] = [4, 5, 6].iter().map((x: int) -> int { x + 1 }).collect()
  if (found == null) {
    []
  } else {
    collected
  }
}

function implicit_flat_map<[CURSOR]() -> int[] {
  let flatMapped<[CURSOR] = baml.iter.ArrayIterator.new([1, 2]).flat_map((x: int) -> baml.iter.ArrayIterator<int> {
    baml.iter.ArrayIterator.new([x, x + 10])
  })
  flatMapped.collect()
}

function implicit_flatten<[CURSOR]() -> int[] {
  let nested<[CURSOR] = baml.iter.ArrayIterator.new([
    baml.iter.ArrayIterator.new([1, 2]),
    baml.iter.ArrayIterator.new([3, 4]),
  ])
  let flattened<[CURSOR] = baml.iter.flatten(nested)
  let flattenMethod<[CURSOR] = flattened.collect
  flattened.collect()
}

function throwing_mapper<[CURSOR]() -> string[] {
  let throwingMapped<[CURSOR] = baml.iter.ArrayIterator.new([1, 2]).map((x: int) -> string throws LocalError {
    if (x == 2) {
      throw LocalError { message: "bad" }
    }
    "ok"
  })
  let collectThrowing<[CURSOR] = throwingMapped.collect
  throwingMapped.collect() catch (e) {
    LocalError => [e.message],
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at stdlib_iterator_inference.baml:5:27
// No hover content
// hover at stdlib_iterator_inference.baml:6:13
// ```baml
// source: baml.iter.ArrayIterator<int>
// ```
// hover at stdlib_iterator_inference.baml:7:17
// ```baml
// nextMethod: () -> int | baml.iter.Done throws never
// ```
// hover at stdlib_iterator_inference.baml:8:16
// ```baml
// mapMethod: !error
// ```
// hover at stdlib_iterator_inference.baml:9:17
// ```baml
// iterMethod: () -> baml.iter.Iterator<Error = never, Item = int> throws never
// ```
// hover at stdlib_iterator_inference.baml:11:13
// ```baml
// mapped: baml.iter.Iterator<Error = never, Item = int>
// ```
// hover at stdlib_iterator_inference.baml:12:15
// ```baml
// filtered: baml.iter.Iterator<Error = never, Item = int>
// ```
// hover at stdlib_iterator_inference.baml:13:19
// ```baml
// filterMapped: baml.iter.Iterator<Error = never, Item = string>
// ```
// hover at stdlib_iterator_inference.baml:21:17
// ```baml
// stringIter: baml.iter.Iterator<Error = never, Item = int>
// ```
// hover at stdlib_iterator_inference.baml:22:14
// ```baml
// stepped: baml.iter.Iterator<Error = never, Item = int>
// ```
// hover at stdlib_iterator_inference.baml:23:16
// ```baml
// collected: int[]
// ```
// hover at stdlib_iterator_inference.baml:27:29
// No hover content
// hover at stdlib_iterator_inference.baml:28:16
// ```baml
// arrayIter: baml.iter.Iterator<Error = never, Item = int>
// ```
// hover at stdlib_iterator_inference.baml:29:12
// ```baml
// found: int | null
// ```
// hover at stdlib_iterator_inference.baml:30:16
// ```baml
// collected: int[]
// ```
// hover at stdlib_iterator_inference.baml:38:27
// No hover content
// hover at stdlib_iterator_inference.baml:39:17
// ```baml
// flatMapped: baml.iter.Iterator<Error = never, Item = int>
// ```
// hover at stdlib_iterator_inference.baml:45:26
// No hover content
// hover at stdlib_iterator_inference.baml:46:13
// ```baml
// nested: baml.iter.ArrayIterator<baml.iter.ArrayIterator<int>>
// ```
// hover at stdlib_iterator_inference.baml:50:16
// ```baml
// flattened: baml.iter.Iterator<Error = never, Item = int>
// ```
// hover at stdlib_iterator_inference.baml:51:20
// ```baml
// flattenMethod: () -> int[] throws never
// ```
// hover at stdlib_iterator_inference.baml:55:25
// No hover content
// hover at stdlib_iterator_inference.baml:56:21
// ```baml
// throwingMapped: baml.iter.Iterator<Error = LocalError, Item = string>
// ```
// hover at stdlib_iterator_inference.baml:62:22
// ```baml
// collectThrowing: () -> string[] throws LocalError
// ```
