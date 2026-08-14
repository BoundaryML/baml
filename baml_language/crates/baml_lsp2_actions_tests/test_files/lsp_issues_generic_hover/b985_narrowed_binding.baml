class Lorem<T> {}

function use(a: int | Lorem<int>) -> void {
  let b: int | Lorem<int> = a
  match (b) {
    int => {
      let c: int = b<[CURSOR]
    }
    Lorem<int> => {}
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// (B-985: hovering b in the int match arm should show the narrowed `b: int` type.)
// hover at b985_narrowed_binding.baml:7:21
// ```baml
// b: int
// ```
