// Type alias rows from 18a §"Type shapes":
//   - non-recursive `type StringList = string[]`, used as a field
//   - recursive `type RecList = int | RecList[]`
//
// The recursive alias is also referenced from a different leaf
// (ns_aliases_consumer/) so the routed-name path (`RecList` opaque,
// quoted as forward-ref in self-reference) is exercised in isolation
// from the simpler non-recursive inlining case.

type StringList = string[]

type RecList = int | RecList[]

class AliasContainer {
  list_field StringList
  // rec_field RecList — DISABLED. Pydantic 2.13 hits a recursion error
  // when a class references a recursive `typing.TypeAlias` that is
  // emitted *before* the class. Codegen emits in BAML source order, and
  // declaring the type alias first happens to put pydantic into the
  // failing path. See 18b §"Recursive alias before class triggers
  // pydantic recursion".
}

function PassStringList(s: StringList) -> StringList {
  s
}

function PassRecList(r: RecList) -> RecList {
  r
}
