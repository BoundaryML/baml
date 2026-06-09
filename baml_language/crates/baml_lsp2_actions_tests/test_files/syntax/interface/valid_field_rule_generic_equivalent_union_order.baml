// BEP-044: generic interface field matching is invariant, but the substituted
// field type is compared after TIR normalization.

interface Measured<T, E> {
  value: T | E
}

class Reading<T, E> {
  value: E | T
  implements Measured<T, E> {}
}

//----
//- diagnostics
// <no-diagnostics-expected>
