// Trivial recursive type.
class Node {
  data int
  next Node?
}

// Mutually recursive types.
class Tree {
  data int
  children Forest
}

class Forest {
  trees Tree[]
}

// Unions.
class BasicUnion {
  data int | BasicUnion
}

class OptionalUnion {
  data OptionalUnion? | OptionalUnion
}

class FullyOptionalUnion {
  data (FullyOptionalUnion | FullyOptionalUnion)?
}

class NestedUnion {
  data NestedUnion | BasicUnion
}

class UnionOfUnions {
  data (Node | (BasicUnion | Tree)) | (NestedUnion | UnionOfUnions) | Forest
}