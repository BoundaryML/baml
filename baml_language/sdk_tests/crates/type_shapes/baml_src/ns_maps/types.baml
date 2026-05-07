// Map rows from 18a §"Type shapes":
//   - `map<string, int>`              → `typing.Dict[str, int]`
//   - `map<Sentiment, Resume>`        → `typing.Dict[Sentiment, Resume]`
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
  enum_keyed map<Sentiment, Resume>
  list_valued map<string, int[]>
}

function PassSimpleMap(m: map<string, int>) -> map<string, int> {
  m
}

function PassEnumKeyedMap(m: map<Sentiment, Resume>) -> map<Sentiment, Resume> {
  m
}

function PassListValuedMap(m: map<string, int[]>) -> map<string, int[]> {
  m
}
