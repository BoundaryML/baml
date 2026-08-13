// Minimal expression-function fixture: a nullary function whose return
// type is the string literal "hello world" and whose body returns that
// same literal. Exercises the host→engine call path for a plain
// (non-LLM) BAML function with no arguments.
function hello_world() -> "hello world" {
  "hello world"
}

// One required positional argument: the host passes a single value and it
// round-trips through the engine unchanged. The one-arg step up from the
// nullary `hello_world`.
function single_required_arg(value: string) -> string {
  value
}

// Exercises structured BAML logs across the host SDK call boundary. The
// bridge renders these events to stderr according to the BAML_LOG threshold.
function sdk_bridge_logs() -> int {
  log.debug("sdk bridge debug")
  log.info("sdk bridge info")
  log.warn({ "kind": "sdk bridge warning" })
  log.error("sdk bridge error")
  42
}

// Detached work can outlive the root SDK call, so its logs exercise the
// bridge's post-result drain lifecycle.
function sdk_bridge_detached_log() -> int {
  spawn with baml.spawn.options(detach = true) {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(100n))
    log.info("sdk bridge detached info")
  }
  42
}

function unhandled_spawn_error() -> int throws string {
  throw "boom"
}

function spawn_unhandled_error() -> int {
  spawn { unhandled_spawn_error() }
  1
}

// First nominal-value bridge proof: the generated Go struct crosses the FFI
// boundary in both directions with exact class and field wire names.
class Person {
  // Projects to the same Go identifier as the surrounding class. Generated
  // decoding must never introduce this field as a local variable.
  person string
  name string
  age int
}

function round_trip_person(person: Person) -> Person {
  person
}

// Primitive identity functions keep this call-boundary smoke test focused:
// every value crosses host -> WASM -> BEX -> WASM -> host without involving
// collection or class encoding.
function round_trip_int(value: int) -> int {
  value
}

function round_trip_bool(value: bool) -> bool {
  value
}

function round_trip_string(value: string) -> string {
  value
}

function round_trip_float(value: float) -> float {
  value
}

// Helper used as an *expression* default below — `opt2`'s default is a call to
// this function, so the engine evaluates it whenever `opt2` is omitted.
function make_opt2() -> int {
  99
}

// arg0 echoes back to confirm the required positional reached the engine;
// opt1 has a literal default; opt2 has an expression default.
function optional_args_probe(arg0: int, opt1: int? = 5, opt2: int? = make_opt2()) -> int?[] {
  [arg0, opt1, opt2]
}

class DefaultArgsMatrixResult {
  string_value string
  int_value int
  bigint_value bigint
  float_value float
  bool_value bool
  null_value null
  bytes_value uint8array?
  class_value Person
  list_value string[]
  map_value map<string, int>
  list_optional string?[]
  map_optional map<string, int?>
  nullable_value string?
  optional_class Person?
  optional_list string[]?
  optional_map map<string, int>?
}

// Every currently supported Go wire shape in a defaulted argument. The Go
// generator must not interpret these defaults; omission lets BAML evaluate
// them, while a generated option supplies an explicit wire value.
function default_args_matrix(
  string_value: string = "default",
  int_value: int = 10,
  bigint_value: bigint = 11n,
  float_value: float = 12.5,
  bool_value: bool = true,
  null_value: null = null,
  bytes_value: uint8array? = null,
  class_value: Person = Person { person: "default", name: "Default", age: 13 },
  list_value: string[] = ["default"],
  map_value: map<string, int> = { "default": 14 },
  list_optional: string?[] = [null, "default"],
  map_optional: map<string, int?> = { "default": null },
  nullable_value: string? = null,
  optional_class: Person? = null,
  optional_list: string[]? = null,
  optional_map: map<string, int>? = null,
) -> DefaultArgsMatrixResult {
  DefaultArgsMatrixResult {
    string_value: string_value,
    int_value: int_value,
    bigint_value: bigint_value,
    float_value: float_value,
    bool_value: bool_value,
    null_value: null_value,
    bytes_value: bytes_value,
    class_value: class_value,
    list_value: list_value,
    map_value: map_value,
    list_optional: list_optional,
    map_optional: map_optional,
    nullable_value: nullable_value,
    optional_class: optional_class,
    optional_list: optional_list,
    optional_map: optional_map,
  }
}

// These declarations normalize to the generated option type and setter names
// for default_name_collisions. Typed package-wide allocation must disambiguate
// all of them without affecting their exact wire names.
class DefaultNameCollisionsOption {}

function with_default_name_collisions_option() -> string {
  "declaration"
}

function default_name_collisions(
  arguments: string,
  options: string = "options",
  option: string = "option",
) -> string[] {
  [arguments, options, option]
}

function defaulted_void(value: string = "default") -> void {}

class OptBox {
  base int

  function probe(self, arg0: int, opt1: int? = 5) -> int?[] {
    [self.base, arg0, opt1]
  }

  function make(base: int, opt1: int? = 7) -> OptBox {
    OptBox { base: base + (opt1 ?? 0) }
  }
}
