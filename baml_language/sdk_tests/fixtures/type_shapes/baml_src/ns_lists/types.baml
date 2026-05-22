// List rows from 18a §"Type shapes":
//   - `int[]`                  → `typing.List[int]`
//   - `string?[]`              → `typing.List[typing.Union[str, None]]`
//   - `(int | string)[]`       → `typing.List[typing.Union[int, str]]`

class ListContainer {
  ints int[]
  optional_strings string?[]
  union_list (int | string)[]
}

function RoundTripInts(xs: int[]) -> int[] {
  xs
}

function RoundTripOptionalStrings(xs: string?[]) -> string?[] {
  xs
}

function RoundTripUnionList(xs: (int | string)[]) -> (int | string)[] {
  xs
}

function RoundTripListContainer(c: ListContainer) -> ListContainer {
  c
}
