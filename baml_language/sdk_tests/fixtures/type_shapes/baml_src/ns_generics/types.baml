// Generic-class rows from 18a §"Type shapes / generics":
//   - recursive generic: GenericLinkedList<T> { value: T, next: GenericLinkedList<T>? }
//   - GenericBinaryTree<T> { value: T, left/right: GenericBinaryTree<T>? }
//   - nested generics: Wrapper<Wrapper<int>>, Wrapper<int[]>
//   - generic with TypeVar arg: inside Box<T>, sibling field Wrapper<T>
//   - nested over recursive: Wrapper<GenericLinkedList<int>>
//   - "instantiation differs from declaration" — inside GenericLinkedList<T>,
//     `next: GenericLinkedList<Wrapper<int>>` (different T arg) → rendered
//     as `Union["GenericLinkedList[Wrapper[int]]", None]`.

class Wrapper<T> {
  value T
}

class GenericLinkedList<T> {
  value T
  next GenericLinkedList<T>?
}

class GenericBinaryTree<T> {
  value T
  left GenericBinaryTree<T>?
  right GenericBinaryTree<T>?
}

class Box<T> {
  value T
  // Sibling field referencing Wrapper<T>: TypeVar `T` flows into a
  // generic-arg position to verify TypeVar renders as a bare identifier.
  wrapped Wrapper<T>
}

class NestedGenerics {
  // Wrapper<Wrapper<int>> — nested generics, two levels deep.
  ww Wrapper<Wrapper<int>>
  // Wrapper over a list — nested over a non-class type.
  wl Wrapper<int[]>
  // Wrapper over a recursive instantiation.
  wr Wrapper<GenericLinkedList<int>>
}

// Differing-instantiation case: a self-typed `GenericLinkedList<Wrapper<int>>`
// field on a non-generic class, so the recursive instantiation site differs
// from any declaration site.
class DifferingInstantiation {
  list GenericLinkedList<Wrapper<int>>
}

// Generic classes can't be passed without a concrete instantiation, so we
// pick `<int>` for each to exercise the lowered specialization.
function RoundTripWrapperInt(w: Wrapper<int>) -> Wrapper<int> {
  w
}

function RoundTripGenericLinkedListInt(l: GenericLinkedList<int>) -> GenericLinkedList<int> {
  l
}

function RoundTripGenericBinaryTreeInt(t: GenericBinaryTree<int>) -> GenericBinaryTree<int> {
  t
}

function RoundTripBoxInt(b: Box<int>) -> Box<int> {
  b
}

function RoundTripNestedGenerics(n: NestedGenerics) -> NestedGenerics {
  n
}

function RoundTripDifferingInstantiation(d: DifferingInstantiation) -> DifferingInstantiation {
  d
}
