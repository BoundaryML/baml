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
//    ·                                    ╰── expected `baml.iter.Iterator<Error = never, Item = string>`, found `baml.iter.ArrayIterator<int>`
//    ╰────
// E0001
//
//   × mismatched types
//     ╭─[iterator_stdlib_iterator_errors.baml:10:43]
//  10 │   baml.iter.ArrayIterator.new([1, 2]).map((x: int) -> string { "not an int" })
//     ·                                           ─────────────────┬─────────────────
//     ·                                                            ╰── expected `(int) -> int throws never`, found `(x: int) -> string throws never`
//     ╰────
// E0006
//
//   × cannot iterate over type `int`
//     ╭─[iterator_stdlib_iterator_errors.baml:14:3]
//  14 │   baml.iter.flatten(baml.iter.ArrayIterator.new([1, 2, 3]))
//     ·   ─────────────────────────────────────────────────────────
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[iterator_stdlib_iterator_errors.baml:13:93]
//  13 │ ╭─▶ function wrong_flatten_item() -> baml.iter.Iterator<Item = int, Error = never> throws never {
//  14 │ │     baml.iter.flatten(baml.iter.ArrayIterator.new([1, 2, 3]))
//  15 │ ├─▶ }
//     · ╰──── expected `baml.iter.Iterator<Error = never, Item = int>`, found `baml.iter.Iterator<Error = !error, Item = int>`
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[iterator_stdlib_iterator_errors.baml:17:95]
//  17 │ ╭─▶ function wrong_error_binding() -> baml.iter.Iterator<Item = int, Error = string> throws never {
//  18 │ │     baml.iter.ArrayIterator.new([1, 2, 3])
//  19 │ ├─▶ }
//     · ╰──── expected `baml.iter.Iterator<Error = string, Item = int>`, found `baml.iter.ArrayIterator<int>`
//     ╰────
