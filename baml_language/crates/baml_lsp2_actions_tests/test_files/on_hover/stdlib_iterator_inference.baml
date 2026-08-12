class LocalError {
  message string
}

function implicit_pipeline<[CURSOR]() -> int[] throws unknown {
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

function implicit_array_iter<[CURSOR]() -> int[] throws unknown {
  let arrayIter<[CURSOR] = [1, 2, 3].iter()
  let found<[CURSOR] = arrayIter.find((x: int) -> bool { x == 2 })
  let collected<[CURSOR] = [4, 5, 6].iter().map((x: int) -> int { x + 1 }).collect()
  if (found == null) {
    []
  } else {
    collected
  }
}

function implicit_flat_map<[CURSOR]() -> int[] throws unknown {
  let flatMapped<[CURSOR] = baml.iter.ArrayIterator.new([1, 2]).flat_map((x: int) -> baml.iter.ArrayIterator<int> {
    baml.iter.ArrayIterator.new([x, x + 10])
  })
  flatMapped.collect()
}

function implicit_flatten<[CURSOR]() -> int[] throws unknown {
  let nested<[CURSOR] = baml.iter.ArrayIterator.new([
    baml.iter.ArrayIterator.new([1, 2]),
    baml.iter.ArrayIterator.new([3, 4]),
  ])
  let flattened<[CURSOR] = baml.iter.flatten(nested)
  let flattenMethod<[CURSOR] = flattened.collect
  flattened.collect()
}

function throwing_mapper<[CURSOR]() -> string[] throws unknown {
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
// E0097
//
//   ⚠ extraneous throws declaration: unknown
//     ╭─[stdlib_iterator_inference.baml:5:54]
//   5 │ ╭─▶ function implicit_pipeline() -> int[] throws unknown {
//   6 │ │     let source = baml.iter.ArrayIterator.new([1, 2, 3, 4])
//   7 │ │     let nextMethod = source.next
//   8 │ │     let mapMethod = source.map
//   9 │ │     let iterMethod = source.iter
//  10 │ │
//  11 │ │     let mapped = source.map((x: int) -> int { x * 2 })
//  12 │ │     let filtered = mapped.filter((x: int) -> bool { x > 2 })
//  13 │ │     let filterMapped = filtered.filter_map((x: int) -> string? {
//  14 │ │       if (x % 2 == 0) {
//  15 │ │         "even"
//  16 │ │       } else {
//  17 │ │         null
//  18 │ │       }
//  19 │ │     })
//  20 │ │
//  21 │ │     let stringIter = filterMapped.map((s: string) -> int { s.length() })
//  22 │ │     let stepped = stringIter.step_by(2)
//  23 │ │     let collected = stepped.collect()
//  24 │ │     collected
//  25 │ ╰─▶ }
//     ╰────
// E0097
//
//   ⚠ extraneous throws declaration: unknown
//     ╭─[stdlib_iterator_inference.baml:27:56]
//  27 │ ╭─▶ function implicit_array_iter() -> int[] throws unknown {
//  28 │ │     let arrayIter = [1, 2, 3].iter()
//  29 │ │     let found = arrayIter.find((x: int) -> bool { x == 2 })
//  30 │ │     let collected = [4, 5, 6].iter().map((x: int) -> int { x + 1 }).collect()
//  31 │ │     if (found == null) {
//  32 │ │       []
//  33 │ │     } else {
//  34 │ │       collected
//  35 │ │     }
//  36 │ ╰─▶ }
//     ╰────
// E0097
//
//   ⚠ extraneous throws declaration: unknown
//     ╭─[stdlib_iterator_inference.baml:38:54]
//  38 │ ╭─▶ function implicit_flat_map() -> int[] throws unknown {
//  39 │ │     let flatMapped = baml.iter.ArrayIterator.new([1, 2]).flat_map((x: int) -> baml.iter.ArrayIterator<int> {
//  40 │ │       baml.iter.ArrayIterator.new([x, x + 10])
//  41 │ │     })
//  42 │ │     flatMapped.collect()
//  43 │ ╰─▶ }
//     ╰────
// E0097
//
//   ⚠ extraneous throws declaration: unknown
//     ╭─[stdlib_iterator_inference.baml:45:53]
//  45 │ ╭─▶ function implicit_flatten() -> int[] throws unknown {
//  46 │ │     let nested = baml.iter.ArrayIterator.new([
//  47 │ │       baml.iter.ArrayIterator.new([1, 2]),
//  48 │ │       baml.iter.ArrayIterator.new([3, 4]),
//  49 │ │     ])
//  50 │ │     let flattened = baml.iter.flatten(nested)
//  51 │ │     let flattenMethod = flattened.collect
//  52 │ │     flattened.collect()
//  53 │ ╰─▶ }
//     ╰────
// E0097
//
//   ⚠ extraneous throws declaration: unknown
//     ╭─[stdlib_iterator_inference.baml:55:55]
//  55 │ ╭─▶ function throwing_mapper() -> string[] throws unknown {
//  56 │ │     let throwingMapped = baml.iter.ArrayIterator.new([1, 2]).map((x: int) -> string throws LocalError {
//  57 │ │       if (x == 2) {
//  58 │ │         throw LocalError { message: "bad" }
//  59 │ │       }
//  60 │ │       "ok"
//  61 │ │     })
//  62 │ │     let collectThrowing = throwingMapped.collect
//  63 │ │     throwingMapped.collect() catch (e) {
//  64 │ │       LocalError => [e.message],
//  65 │ │     }
//  66 │ ╰─▶ }
//     ╰────
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
