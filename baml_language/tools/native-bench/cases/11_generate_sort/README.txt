Generate numbers and use the library sort

Question
  What does a common generate-sort-serialize task cost through each runtime's
  normal numeric sorting API, including BAML's native integer-sort path?
Work
  Starting with seed 1729, repeat x = (x * 48271) mod 1000000007, append each x,
  sort ascending using the runtime library, and serialize every result.
Boundary
  Only count and seed are prepared. Generation, allocation, sort and JSON
  encoding are timed. Products stay below 2^53 for the documented seed range,
  so the JavaScript number implementations preserve these integer operations.
  Supported seeds are integers from 0 through 1000000006 inclusive.
BAML path
  VM arithmetic and array growth, Sortable.sort dispatch, native _rust_sort,
  then native JSON encoding. Integer arrays avoid per-comparison VM callbacks.
Evidence
  The oracle derives x_i with modular exponentiation rather than copying the
  recurrence. Cases cover empty, singleton, zero seed and the upper seed bound.
Claim
  Same task and values, different library algorithms. This must not be presented
  as the handwritten merge/quicksort experiment with only an algorithm changed.
References
  https://github.com/BoundaryML/baml/blob/340b9085d9427e6d458f1c700409ffc88c58b050/baml_language/crates/baml_builtins2/baml_std/baml/sortable.baml
  https://doc.rust-lang.org/std/primitive.slice.html#method.sort
  https://pkg.go.dev/slices#Sort
  https://docs.python.org/3/library/stdtypes.html#list.sort
