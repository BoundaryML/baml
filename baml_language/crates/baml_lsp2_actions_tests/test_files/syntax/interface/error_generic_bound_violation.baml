// BEP-044 §"Generic bounds": passing a `T` that doesn't implement the bound
// interface is a compile error.

interface Named {
  name: string
}

function first_named<T extends Named>(items: T[]) -> string {
  return items[0].name
}

function bad() -> string {
  let ints: int[] = [1, 2, 3]
  return first_named<int>(ints)
}

//----
//- diagnostics
// error: type mismatch: expected Named, got int
//     ╭─[ interface_error_generic_bound_violation.baml:14:9 ]
//     │
//  14 │   return first_named<int>(ints)
//     │         ───────────┬───────────  
//     │                    ╰───────────── type mismatch: expected Named, got int
//     │ 
//     │ Note: Error code: E0001
// ────╯
