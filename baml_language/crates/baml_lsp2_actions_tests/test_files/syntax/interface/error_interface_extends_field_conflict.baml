// BEP-044 §"Requires with conflicting field types": the error is on the child
// interface itself, not on implementors.

interface X {
  id: string
}

interface Y {
  id: int
}

interface Z requires X, Y {}

//----
//- diagnostics
// <no-diagnostics-expected>
