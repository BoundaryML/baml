class Bar {
  bar_inner int
}

class Baz {
  baz_inner string
}

function Foo() -> (Bar | Baz)[] {
  [Bar{ bar_inner: 1}]
}

//>Foo()
//
// [
//   {
//     "bar_inner": 1
//   }
// ]
