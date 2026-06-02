// Forward-reference rows from 18a §"Type shapes / forward references":
//   - alias self-ref bare:                 inside `RecList`, `RecList`     → `"RecList"`
//   - alias self-ref nested in list:       inside `RecList`, `RecList[]`   → `List["RecList"]`
//   - alias self-ref nested in union:      same alias's union element       → `List["RecList"]`
//   - class self-ref bare:                 inside `Node`, `Node`            → `"Node"`
//   - generic class self-ref preserves args: inside `GNode<T>`, `GNode<T>[]` → `"Node[T]"`
//   - non-self ref stays unquoted:         inside `RecListWithOther`, bare
//                                          `Other` renders as `Other` (only
//                                          the *enclosing* alias gets the quote)

class Other {
  v int
}

// Self-ref bare + nested in list (`RecList` covers both — same body) and in
// union (the union arm `RecList[]` is itself a list of self-refs).
type RecList = int | RecList[]

// "non-self ref stays unquoted": Other appears unquoted, RecListWithOther
// itself appears quoted in the list arm.
type RecListWithOther = int | Other | RecListWithOther[]

// Class self-ref bare. Codegen quotes the `next` field's type as `"Node"`.
class Node {
  next Node
}

// Generic self-ref preserves args inside the quotes — `GNode<T>[]` →
// list element `"Node[T]"`. We name the class `GNode` to keep it
// distinct from the non-generic `Node` above.
class GNode<T> {
  children GNode<T>[]
}

function round_trip_other(o: Other) -> Other {
  o
}

function round_trip_rec_list(r: RecList) -> RecList {
  r
}

function round_trip_rec_list_with_other(r: RecListWithOther) -> RecListWithOther {
  r
}

function round_trip_node(n: Node) -> Node {
  n
}

function round_trip_g_node_int(g: GNode<int>) -> GNode<int> {
  g
}
