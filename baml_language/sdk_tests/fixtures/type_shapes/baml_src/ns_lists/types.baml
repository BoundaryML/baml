// List rows from 18a §"Type shapes":
//   - `int[]`                  → `typing.List[int]`
//   - `string?[]`              → `typing.List[typing.Union[str, None]]`
//   - `(int | string)[]`       → `typing.List[typing.Union[int, str]]`

class ListContainer {
  ints int[]
  optional_strings string?[]
  union_list (int | string)[]
}

function round_trip_ints(xs: int[]) -> int[] {
  xs
}

function round_trip_optional_strings(xs: string?[]) -> string?[] {
  xs
}

function round_trip_union_list(xs: (int | string)[]) -> (int | string)[] {
  xs
}

function round_trip_list_container(c: ListContainer) -> ListContainer {
  c
}
