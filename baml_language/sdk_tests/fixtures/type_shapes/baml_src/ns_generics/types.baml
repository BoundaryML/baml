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
function round_trip_wrapper_int(w: Wrapper<int>) -> Wrapper<int> {
  w
}

function round_trip_generic_linked_list_int(l: GenericLinkedList<int>) -> GenericLinkedList<int> {
  l
}

function round_trip_generic_binary_tree_int(t: GenericBinaryTree<int>) -> GenericBinaryTree<int> {
  t
}

function round_trip_box_int(b: Box<int>) -> Box<int> {
  b
}

function round_trip_nested_generics(n: NestedGenerics) -> NestedGenerics {
  n
}

function round_trip_differing_instantiation(d: DifferingInstantiation) -> DifferingInstantiation {
  d
}
