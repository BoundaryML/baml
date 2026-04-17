class Formatter {
  prefix string
  function format(self, text: string) -> string { self.prefix + text }
}

class Counter {
  value int
  function increment(self) -> int { self.value += 1; self.value }
}

class Employee {
  first string
  last string
  function full_name(self) -> string { self.first + " " + self.last }
}

// Error 1: Extra arg on bound ref
function test_err_extra_arg() -> string {
  let fmt = Formatter { prefix: ">> " };
  let transform = fmt.format;
  transform(fmt, "hello")
}

// Error 2: Missing arg on unbound ref
function test_err_missing_self() -> string {
  let f = Employee.full_name;
  f()
}

// Error 3: Bound type assigned to wrong function type
function test_err_bound_type_mismatch() -> int {
  let c = Counter { value: 0 };
  let inc: (Counter) -> int = c.increment;
  0
}

// Error 4: Unbound type assigned to zero-arg function type
function test_err_unbound_type_mismatch() -> string {
  let f: () -> string = Employee.full_name;
  ""
}

// Error 5: Bound passed where unbound expected
function takes_unbound(f: (Employee) -> string) -> string {
  let e = Employee { first: "X", last: "Y" };
  f(e)
}
function test_err_bound_for_unbound() -> string {
  let e = Employee { first: "Alice", last: "Smith" };
  takes_unbound(e.full_name)
}
