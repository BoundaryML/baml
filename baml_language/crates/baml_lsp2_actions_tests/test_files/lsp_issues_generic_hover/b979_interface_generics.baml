interface Foo<T> {
  function value(self) -> T throws never
}

function use(foo: Foo<int>) -> int {
  let ignored = F<[CURSOR]oo
  return foo.value()
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// (B-979: hovering an interface reference should include its generic parameters.)
// hover at b979_interface_generics.baml:6:18
// ```baml
// interface Foo<T>
// ```
