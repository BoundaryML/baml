class Formatter {
  prefix string
  function format(self, text: string) -> string { self.prefix + text }
}

class RangeCheck {
  min int
  max int
  function is_valid(self, n: int) -> bool { n >= self.min && n <= self.max }
}

class Counter {
  value int
  function increment(self) -> int { self.value += 1; self.value }
}

class Employee {
  first string
  last string
  function full_name(self) -> string { self.first + " " + self.last }
  function greet(self, greeting: string) -> string { greeting + ", " + self.first }
}

class Config {
  encoder Encoder
}

class Encoder {
  charset string
  function encode(self, data: string) -> string { "[" + self.charset + "]" + data }
}

class Box<T> {
  value T
  function unwrap(self) -> T { self.value }
}

class Calculator {
  op string
  function compute(self, a: int, b: int) -> int {
    if self.op == "add" { a + b } else { a * b }
  }
}

// Test 1: Bound formatter as mapper
function test_bound_mapper() -> string[] {
  let fmt = Formatter { prefix: ">> " };
  let transform = fmt.format;
  ["hello", "world"].map(transform)
}

// Test 2: Unbound method in map
function test_unbound_in_map() -> string[] {
  let employees = [
    Employee { first: "Alice", last: "Smith" },
    Employee { first: "Bob", last: "Jones" },
  ];
  employees.map(Employee.full_name)
}

// Test 3: Bound validator mapped over data
function test_bound_validator() -> bool[] {
  let valid = RangeCheck { min: 10, max: 50 };
  let check = valid.is_valid;
  [5, 15, 25, 75].map(check)
}

// Test 4: Bound through chained field access
function test_field_chain_bind(cfg: Config) -> string {
  let enc = cfg.encoder.encode;
  enc("payload")
}

// Test 5: Strategy pattern
function apply(f: (int, int) -> int, a: int, b: int) -> int { f(a, b) }

function test_strategy() -> int[] {
  let add_calc = Calculator { op: "add" };
  let mul_calc = Calculator { op: "mul" };
  let do_add = add_calc.compute;
  let do_mul = mul_calc.compute;
  [apply(do_add, 3, 4), apply(do_mul, 3, 4)]
}

// Test 6: Unbound method passed to higher-order function
function apply_to_each(items: Employee[], f: (Employee) -> string) -> string[] {
  items.map(f)
}

function test_unbound_higher_order() -> string[] {
  let employees = [Employee { first: "Alice", last: "Smith" }];
  apply_to_each(employees, Employee.full_name)
}

// Test 7: Mutation after bind — capture-by-reference
function test_mutation_after_bind() -> int {
  let c = Counter { value: 0 };
  let inc = c.increment;
  c.value = 100;
  inc()
}

// Test 8: Generic class — type params flow through bind
function test_generic_bind() -> int {
  let b = Box { value: 42 };
  let get = b.unwrap;
  get()
}

// Test 9: Bound method returned from a lambda
function test_bound_in_lambda() -> string {
  let emp = Employee { first: "Alice", last: "Smith" };
  let make_greeter = () -> { emp.greet };
  let greeter = make_greeter();
  greeter("Hello")
}

// Test 10: Repeated calls on same bound ref
function test_repeated_bound_calls() -> int {
  let c = Counter { value: 0 };
  let inc = c.increment;
  inc();
  inc();
  inc()
}
