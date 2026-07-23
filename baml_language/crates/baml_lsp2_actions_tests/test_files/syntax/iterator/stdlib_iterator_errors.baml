function takes_strings(it: baml.iter.Iterator<Item = string, Error = never>) -> string[] throws never {
  it.collect()
}

function wrong_iterator_item() -> string[] throws never {
  takes_strings(baml.iter.ArrayIterator.new([1, 2, 3]))
}

function wrong_map_result() -> baml.iter.Iterator<Item = int, Error = never> throws never {
  baml.iter.ArrayIterator.new([1, 2]).map((x: int) -> string { "not an int" })
}

function wrong_flatten_item() -> baml.iter.Iterator<Item = int, Error = never> throws never {
  baml.iter.flatten(baml.iter.ArrayIterator.new([1, 2, 3]))
}

function wrong_error_binding() -> baml.iter.Iterator<Item = int, Error = string> throws never {
  baml.iter.ArrayIterator.new([1, 2, 3])
}

//----
//- diagnostics
// E0001
//
//   × mismatched types
//    ╭─[iterator_stdlib_iterator_errors.baml:6:17]
//  6 │   takes_strings(baml.iter.ArrayIterator.new([1, 2, 3]))
//    ·                 ───────────────────┬──────────────────
//    ·                                    ╰── expected `baml.iter.Iterator<Item = string, Error = never>`, found `baml.iter.ArrayIterator<int>`
//    ╰────
// E0001
//
//   × mismatched types
//     ╭─[iterator_stdlib_iterator_errors.baml:10:43]
//  10 │   baml.iter.ArrayIterator.new([1, 2]).map((x: int) -> string { "not an int" })
//     ·                                           ─────────────────┬─────────────────
//     ·                                                            ╰── expected `(int) -> int throws never`, found `(x: int) -> string throws never`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[iterator_stdlib_iterator_errors.baml:10:3]
//  10 │   baml.iter.ArrayIterator.new([1, 2]).map((x: int) -> string { "not an int" })
//     ·   ──────────────────────────────────────┬─────────────────────────────────────
//     ·                                         ╰── expected `baml.iter.Iterator<Item = int, Error = never>`, found `baml.iter.Iterator<Item = int | string, Error = never>`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[iterator_stdlib_iterator_errors.baml:14:3]
//  14 │   baml.iter.flatten(baml.iter.ArrayIterator.new([1, 2, 3]))
//     ·   ────────────────────────────┬────────────────────────────
//     ·                               ╰── expected `baml.iter.Iterable<Error = E3, Item = int>`, found `int`
//     ╰────
// E0002
//
//   × cannot infer type parameter `E3`
//     ╭─[iterator_stdlib_iterator_errors.baml:14:3]
//  14 │   baml.iter.flatten(baml.iter.ArrayIterator.new([1, 2, 3]))
//     ·   ─────────────────────────────────────────────────────────
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[iterator_stdlib_iterator_errors.baml:18:3]
//  18 │   baml.iter.ArrayIterator.new([1, 2, 3])
//     ·   ───────────────────┬──────────────────
//     ·                      ╰── expected `baml.iter.Iterator<Item = int, Error = string>`, found `baml.iter.ArrayIterator<int>`
//     ╰────
