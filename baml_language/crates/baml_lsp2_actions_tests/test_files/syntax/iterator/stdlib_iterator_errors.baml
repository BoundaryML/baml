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
// error: type mismatch: expected baml.iter.Iterator<Item = string, Error = never>, got baml.iter.ArrayIterator<int>
//    ╭─[ iterator_stdlib_iterator_errors.baml:6:17 ]
//    │
//  6 │   takes_strings(baml.iter.ArrayIterator.new([1, 2, 3]))
//    │                 ───────────────────┬──────────────────
//    │                                    ╰──────────────────── type mismatch: expected baml.iter.Iterator<Item = string, Error = never>, got baml.iter.ArrayIterator<int>
//    │
//    │ Note: Error code: E0001
// ───╯
// error: type mismatch: expected (int) -> int throws never, got (x: int) -> string throws never
//     ╭─[ iterator_stdlib_iterator_errors.baml:10:43 ]
//     │
//  10 │   baml.iter.ArrayIterator.new([1, 2]).map((x: int) -> string { "not an int" })
//     │                                           ─────────────────┬─────────────────
//     │                                                            ╰─────────────────── type mismatch: expected (int) -> int throws never, got (x: int) -> string throws never
//     │
//     │ Note: Error code: E0001
// ────╯
// error: type mismatch: expected baml.iter.Iterator<Item = int, Error = never>, got baml.iter.Iterator<Item = int | string, Error = never>
//     ╭─[ iterator_stdlib_iterator_errors.baml:9:92 ]
//     │
//   9 │ ╭─▶ function wrong_map_result() -> baml.iter.Iterator<Item = int, Error = never> throws never {
//  10 │ ├─▶   baml.iter.ArrayIterator.new([1, 2]).map((x: int) -> string { "not an int" })
//     │ │
//     │ ╰──────────────────────────────────────────────────────────────────────────────────── type mismatch: expected baml.iter.Iterator<Item = int, Error = never>, got baml.iter.Iterator<Item = int | string, Error = never>
//     │
//     │     Note: Error code: E0001
// ────╯
// error: type mismatch: expected baml.iter.Iterable<Item = int, Error = E3>, got int
//     ╭─[ iterator_stdlib_iterator_errors.baml:13:94 ]
//     │
//  13 │ ╭─▶ function wrong_flatten_item() -> baml.iter.Iterator<Item = int, Error = never> throws never {
//  14 │ ├─▶   baml.iter.flatten(baml.iter.ArrayIterator.new([1, 2, 3]))
//     │ │
//     │ ╰───────────────────────────────────────────────────────────────── type mismatch: expected baml.iter.Iterable<Item = int, Error = E3>, got int
//     │
//     │     Note: Error code: E0001
// ────╯
// error: cannot infer type parameter `E3`
//     ╭─[ iterator_stdlib_iterator_errors.baml:13:94 ]
//     │
//  13 │ ╭─▶ function wrong_flatten_item() -> baml.iter.Iterator<Item = int, Error = never> throws never {
//  14 │ ├─▶   baml.iter.flatten(baml.iter.ArrayIterator.new([1, 2, 3]))
//     │ │
//     │ ╰───────────────────────────────────────────────────────────────── cannot infer type parameter `E3`
//     │
//     │     Note: Error code: E0002
// ────╯
// error: type mismatch: expected baml.iter.Iterator<Item = int, Error = string>, got baml.iter.ArrayIterator<int>
//     ╭─[ iterator_stdlib_iterator_errors.baml:17:96 ]
//     │
//  17 │ ╭─▶ function wrong_error_binding() -> baml.iter.Iterator<Item = int, Error = string> throws never {
//  18 │ ├─▶   baml.iter.ArrayIterator.new([1, 2, 3])
//     │ │
//     │ ╰────────────────────────────────────────────── type mismatch: expected baml.iter.Iterator<Item = int, Error = string>, got baml.iter.ArrayIterator<int>
//     │
//     │     Note: Error code: E0001
// ────╯
