function echo(value: string) -> string {
  value
}

function round_trip_bool(value: bool) -> bool {
  value
}

function round_trip_int(value: int) -> int {
  value
}

function round_trip_bigint(value: bigint) -> bigint {
  value
}

function round_trip_float(value: float) -> float {
  value
}

function round_trip_bytes(value: uint8array) -> uint8array {
  value
}

function round_trip_null(value: null) -> null {
  value
}

function round_trip_unknown(value: unknown) -> unknown {
  value
}

type ProbeId = int
type ProbeNames = string[]

function round_trip_probe_id(value: ProbeId) -> ProbeId {
  value
}

function round_trip_probe_names(value: ProbeNames) -> ProbeNames {
  value
}

function generic_identity<T>(value: T) -> T {
  value
}

function generic_choose<T>(left: T, right: T) -> T {
  if (true) { left } else { right }
}

function generic_type_name<T>() -> string {
  reflect.type_of<T>().to_string()
}

function generic_optional_identity<T>(value: T?) -> T? {
  value
}

function generic_defaulted_optional<T>(value: T? = null) -> T? {
  value
}

function generic_optional_list<T>(value: T?[]) -> T?[] {
  value
}

function round_trip_optional_int(value: int?) -> int? {
  value
}

function round_trip_int_or_string(value: int | string) -> int | string {
  value
}

function round_trip_int_string_or_bool(
  value: int | string | bool,
) -> int | string | bool {
  value
}

function round_trip_optional_union(
  value: (int | string)?,
) -> (int | string)? {
  value
}

function round_trip_list_or_string(value: int[] | string) -> int[] | string {
  value
}

function round_trip_literal_union(value: 42 | "literal") -> 42 | "literal" {
  value
}

function round_trip_image(value: image) -> image {
  value
}

function round_trip_audio(value: audio) -> audio {
  value
}

function round_trip_video(value: video) -> video {
  value
}

function round_trip_pdf(value: pdf) -> pdf {
  value
}

function add_with_default(value: int, amount: int = 5) -> int {
  value + amount
}

function round_trip_literal_42(value: 42) -> 42 {
  value
}

function sleep_ms(ms: int) -> null throws baml.errors.Io {
  baml.sys.sleep(baml.time.Duration.from_nanoseconds(ms * 1000000n))
}

function round_trip_int_list(value: int[]) -> int[] {
  value
}

function round_trip_optional_int_list(value: int?[]) -> int?[] {
  value
}

function round_trip_string_int_map(value: map<string, int?>) -> map<string, int?> {
  value
}

function round_trip_nested_collection(
  value: map<string, int?[]>,
) -> map<string, int?[]> {
  value
}

class ProbeError {
  code int
  detail string
}

function throw_probe_error() -> int throws ProbeError {
  throw ProbeError { code: 42, detail: "probe failure" }
}

function panic_probe(message: string) -> int {
  baml.sys.panic(message)
}

function exit_probe(code: int) -> int {
  baml.sys.exit(code)
}

enum ProbeLabel {
  Good,
  Bad,
}

class ProbeModel {
  name string
  count int?
  label ProbeLabel
  tags string[]
  scores map<string, int>

  function create(name: string) -> ProbeModel {
    ProbeModel {
      name: name,
      count: null,
      label: ProbeLabel.Good,
      tags: [],
      scores: {},
    }
  }

  function who(self) -> string {
    self.name
  }

  function echo(self, value: string) -> string {
    value
  }

  function add_to_count(self, delta: int = 1) -> int {
    (self.count ?? 0) + delta
  }
}

class EmptyProbe {}

class GenericProbe<T> {
  value T
  history T[]

  function create(value: T) -> GenericProbe<T> {
    GenericProbe<T> { value: value, history: [value] }
  }

  function echo(self, value: T) -> T {
    value
  }

  function convert<U>(self, value: U) -> U {
    value
  }
}

function round_trip_probe_label(value: ProbeLabel) -> ProbeLabel {
  value
}

function round_trip_probe_model(value: ProbeModel) -> ProbeModel {
  value
}

function round_trip_label_or_model(
  value: ProbeLabel | ProbeModel,
) -> ProbeLabel | ProbeModel {
  value
}

function round_trip_empty_probe(value: EmptyProbe) -> EmptyProbe {
  value
}

function round_trip_generic_probe<T>(value: GenericProbe<T>) -> GenericProbe<T> {
  value
}
