// Class composition refs from 18a §"Type shapes":
//   - bare same-namespace ref: `class Outer { inner: Inner }`
// Cross-namespace and root-leaf-shortcut refs live under ns_routing/.

class Inner {
  value int
}

class Outer {
  inner Inner
}

function MakeOuter(value: int) -> Outer {
  Outer { inner: Inner { value: value } }
}

function RoundTripInner(i: Inner) -> Inner {
  i
}

function RoundTripOuter(o: Outer) -> Outer {
  o
}
