interface Foo<T> {
  function value(self) -> T throws never
}

class Lore<[CURSOR]m<T extends Foo<int>> {}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// (B-980: hovering a generic class should include each type parameter's bound.)
// hover at b980_generic_bounds.baml:5:11
// ```baml
// class Lorem<T extends Foo<int>> {}
// ```
