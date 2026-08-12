class Foo {
  a int
  b string
  c float
  d bool
}

class Bar {
  ref Foo
  items int[]
  nested Foo[]
  mapping map<string, int>
  opt int?
}
