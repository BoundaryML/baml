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

class OptBox {
  base int

  function probe(self, arg0: int, opt1: int? = 5) -> int?[] {
    [self.base, arg0, opt1]
  }

  function make(base: int, opt1: int? = 7) -> OptBox {
    OptBox { base: base + (opt1 ?? 0) }
  }
}
