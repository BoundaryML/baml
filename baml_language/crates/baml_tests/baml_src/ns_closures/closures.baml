// Tests for bound and unbound method closure types.
//
// Converted from crates/baml_tests/tests/closures.rs. Each Rust `main()` body
// is moved into a top-level helper function to avoid the VM test-block local
// boxing bug (locals inside a `test` block compare unequal to literals; locals
// inside a `function` are not boxed).
//
// Class names are prefixed with "Cls" to avoid collision with ns_assignments
// (which owns `Counter`).

// ─── bound_formatter_mapper ───────────────────────────────────────────────────

class ClsFormatter {
  prefix string
  function format(self, text: string) -> string { self.prefix + text }
}

function closures_bound_formatter_mapper() -> string[] {
  let fmt = ClsFormatter { prefix: ">> " };
  let transform = fmt.format;
  ["a", "b"].map(transform)
}

test "bound_formatter_mapper" {
    assert.equal(closures_bound_formatter_mapper(), [">> a", ">> b"])
}

// ─── unbound_in_map ───────────────────────────────────────────────────────────

class ClsEmployee {
  first string
  last string
  function full_name(self) -> string { self.first + " " + self.last }
}

function closures_unbound_in_map() -> string[] {
  let employees = [
    ClsEmployee { first: "Alice", last: "Smith" },
    ClsEmployee { first: "Bob", last: "Jones" },
  ];
  employees.map(ClsEmployee.full_name)
}

test "unbound_in_map" {
    assert.equal(closures_unbound_in_map(), ["Alice Smith", "Bob Jones"])
}

// ─── bound_validator_mapped ───────────────────────────────────────────────────

class ClsRangeCheck {
  min int
  max int
  function is_valid(self, n: int) -> bool { n >= self.min && n <= self.max }
}

function closures_bound_validator_mapped() -> bool[] {
  let valid = ClsRangeCheck { min: 10, max: 50 };
  let check = valid.is_valid;
  [5, 15, 25, 75].map(check)
}

test "bound_validator_mapped" {
    assert.equal(closures_bound_validator_mapped(), [false, true, true, false])
}

// ─── mutation_after_bind ──────────────────────────────────────────────────────

// Uses a separate Counter class to avoid collision with ns_assignments.Counter.
class ClsCounter {
  value int
  function increment(self) -> int { self.value += 1; self.value }
}

function closures_mutation_after_bind() -> int {
  let c = ClsCounter { value: 0 };
  let inc = c.increment;
  c.value = 100;
  inc()
}

test "mutation_after_bind" {
    assert.equal(closures_mutation_after_bind(), 101)
}

// ─── repeated_bound_calls ─────────────────────────────────────────────────────

function closures_repeated_bound_calls() -> int {
  let c = ClsCounter { value: 0 };
  let inc = c.increment;
  inc();
  inc();
  inc()
}

test "repeated_bound_calls" {
    assert.equal(closures_repeated_bound_calls(), 3)
}

// ─── field_chain_bind ─────────────────────────────────────────────────────────

class ClsEncoder {
  charset string
  function encode(self, data: string) -> string { "[" + self.charset + "]" + data }
}

class ClsConfig {
  encoder ClsEncoder
}

function closures_field_chain_bind() -> string {
  let cfg = ClsConfig { encoder: ClsEncoder { charset: "utf8" } };
  let enc = cfg.encoder.encode;
  enc("hi")
}

test "field_chain_bind" {
    assert.equal(closures_field_chain_bind(), "[utf8]hi")
}

// ─── strategy_swap ────────────────────────────────────────────────────────────

class ClsCalculator {
  op string
  function compute(self, a: int, b: int) -> int {
    if self.op == "add" { a + b } else { a * b }
  }
}

function closures_apply(f: (int, int) -> int, a: int, b: int) -> int { f(a, b) }

function closures_strategy_swap() -> int[] {
  let add_calc = ClsCalculator { op: "add" };
  let mul_calc = ClsCalculator { op: "mul" };
  let do_add = add_calc.compute;
  let do_mul = mul_calc.compute;
  [closures_apply(do_add, 3, 4), closures_apply(do_mul, 3, 4)]
}

test "strategy_swap" {
    assert.equal(closures_strategy_swap(), [7, 12])
}

// ─── generic_unwrap ───────────────────────────────────────────────────────────

class ClsBox<T> {
  value T
  function unwrap(self) -> T { self.value }
}

function closures_generic_unwrap() -> int {
  let b = ClsBox { value: 42 };
  let get = b.unwrap;
  get()
}

test "generic_unwrap" {
    assert.equal(closures_generic_unwrap(), 42)
}

// ─── bound_in_lambda ──────────────────────────────────────────────────────────

// Separate class (ClsEmployee2) with a greet method, for the bound-method tests.
class ClsEmployee2 {
  first string
  last string
  function greet(self, greeting: string) -> string { greeting + ", " + self.first }
}

function closures_bound_in_lambda() -> string {
  let emp = ClsEmployee2 { first: "Alice", last: "Smith" };
  let make_greeter = () -> { emp.greet };
  let greeter = make_greeter();
  greeter("Hello")
}

test "bound_in_lambda" {
    assert.equal(closures_bound_in_lambda(), "Hello, Alice")
}

// ─── bound_throwing_method_reference_catches_error ───────────────────────────

class ClsWorker {
  factor int
  function risky(self, value: int) -> int throws string {
    if (value < 0) { throw "negative" }
    self.factor * value
  }
}

function closures_bound_throwing_method_reference_catches_error() -> int {
  let worker = ClsWorker { factor: 2 };
  let run = worker.risky;
  run(-1) catch (e) {
    "negative" => -1,
    _ => -2
  }
}

test "bound_throwing_method_reference_catches_error" {
    assert.equal(closures_bound_throwing_method_reference_catches_error(), -1)
}
