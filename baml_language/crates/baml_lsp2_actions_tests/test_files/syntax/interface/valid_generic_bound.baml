// BEP-044 §"Generic bounds with extends": a generic parameter constrained by
// an interface can access that interface's fields/methods.

interface Named {
  name: string
}

class Dog {
  name: string

  implements Named {}
}

function first_named<T extends Named>(items: T[]) -> string {
  return items[0].name
}

//----
//- diagnostics
// <no-diagnostics-expected>
