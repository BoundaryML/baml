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

function RoundTripSimpleMap(m: map<string, int>) -> map<string, int> {
  m
}

function RoundTripEnumKeyedMap(m: map<Sentiment, Resume>) -> map<Sentiment, Resume> {
  m
}

function RoundTripListValuedMap(m: map<string, int[]>) -> map<string, int[]> {
  m
}

function RoundTripSentiment(s: Sentiment) -> Sentiment {
  s
}

function RoundTripResume(r: Resume) -> Resume {
  r
}

function RoundTripMapContainer(c: MapContainer) -> MapContainer {
  c
}
