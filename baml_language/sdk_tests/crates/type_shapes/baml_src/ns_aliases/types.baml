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
  // 18b §4 → 18c: this field used to break pydantic's schema build
  // because the recursive `typing.TypeAlias` was emitted before the
  // class and pydantic recursed forever resolving the alias. The
  // codegen now emits recursive aliases via
  // `typing_extensions.TypeAliasType`, which pydantic resolves through
  // its definitions / `$ref` machinery instead of inlining.
  rec_field RecList
}

function RoundTripStringList(s: StringList) -> StringList {
  s
}

function RoundTripRecList(r: RecList) -> RecList {
  r
}

function RoundTripAliasContainer(c: AliasContainer) -> AliasContainer {
  c
}
