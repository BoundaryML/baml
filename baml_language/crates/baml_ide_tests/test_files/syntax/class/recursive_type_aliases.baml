// Simple alias that points to recursive type.
class Node {
    value int
    next Node?
}

type LinkedList = Node

// Mutual recursion. There is no "type" here at all.
type One = Two

type Two = One

// Cycle. Same as above but longer.
type A = B

type B = C

type C = A

// Recursive class with alias pointing to itself.
class Recursive {
    value int
    ptr RecAlias
}

type RecAlias = Recursive

// Same but finite.
class FiniteRecursive {
    value int
    ptr FiniteRecAlias?
}

type FiniteRecAlias = FiniteRecursive

// Move the "finite" condition to the alias itself. Should still work.
class RecursiveWithOptionalAlias {
    value int
    ptr RecOptionalAlias
}

type RecOptionalAlias = RecursiveWithOptionalAlias?

// Class that points to alias which enters infinite cycle.
class InfiniteCycle {
    value int
    ptr EnterCycle
}

type EnterCycle = NoStop

type NoStop = EnterCycle

// RecursiveMap
type Map = map<string, Map>

// error: Error validating: These aliases form a dependency cycle: One -> Two
//   -->  class/recursive_type_aliases.baml:10
//    | 
//  9 | // Mutual recursion. There is no "type" here at all.
// 10 | type One = Two
//    | 
// error: Error validating: These aliases form a dependency cycle: A -> B -> C
//   -->  class/recursive_type_aliases.baml:15
//    | 
// 14 | // Cycle. Same as above but longer.
// 15 | type A = B
//    | 
// error: Error validating: These aliases form a dependency cycle: EnterCycle -> NoStop
//   -->  class/recursive_type_aliases.baml:51
//    | 
// 50 | 
// 51 | type EnterCycle = NoStop
//    | 
// error: Error validating: These classes form a dependency cycle: Recursive
//   -->  class/recursive_type_aliases.baml:22
//    | 
// 21 | // Recursive class with alias pointing to itself.
// 22 | class Recursive {
// 23 |     value int
// 24 |     ptr RecAlias
// 25 | }
//    |

//----
//- diagnostics
// <no-diagnostics-expected>
