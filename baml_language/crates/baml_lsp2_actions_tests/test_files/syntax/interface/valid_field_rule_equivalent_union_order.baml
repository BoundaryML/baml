// BEP-044: interface field matching is invariant, but exact type equality is
// canonical rather than textual. Union order does not create a different type.

interface Measured {
  value: int | string
}

class Reading {
  value: string | int
  implements Measured {}
}

//----
//- diagnostics
// <no-diagnostics-expected>
