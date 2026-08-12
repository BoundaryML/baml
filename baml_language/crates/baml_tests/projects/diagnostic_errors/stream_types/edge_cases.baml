class SelfRef {
  name string
  children SelfRef[]
}

class AllMustExist {
  a int @stream.must_exist
  b string @stream.must_exist
  c float @stream.must_exist
}

class AllDone {
  a int @stream.done
  b string @stream.done
  c EdgeFoo @stream.done
}

class EdgeFoo {
  x int
}

class NeverFields {
  lit_int 42
  lit_str "fixed"
  lit_bool true
}

class MustExistClass {
  v int
  @@stream.must_exist
}

class UnionWithMustExist {
  mixed MustExistClass | 5
}
