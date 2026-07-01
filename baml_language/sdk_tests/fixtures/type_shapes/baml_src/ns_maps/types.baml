// Map rows from 18a §"Type shapes":
//   - `map<string, int>`              → `typing.Dict[str, int]`
//   - `map<string, Resume>`           → `typing.Dict[str, Resume]`
//   - `map<string, int[]>`            → `typing.Dict[str, typing.List[int]]`

enum Sentiment {
  Positive,
  Negative,
}

class Resume {
  name string
}

class MapContainer {
  simple map<string, int>
  enum_keyed map<string, Resume>
  list_valued map<string, int[]>
}

function round_trip_simple_map(m: map<string, int>) -> map<string, int> {
  m
}

function round_trip_enum_keyed_map(m: map<string, Resume>) -> map<string, Resume> {
  m
}

function round_trip_list_valued_map(m: map<string, int[]>) -> map<string, int[]> {
  m
}

function round_trip_sentiment(s: Sentiment) -> Sentiment {
  s
}

function round_trip_resume(r: Resume) -> Resume {
  r
}

function round_trip_map_container(c: MapContainer) -> MapContainer {
  c
}
