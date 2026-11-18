// Throwing, catching and `defer`. Recursion keeps a throw from being
// inlined into the function that catches it, so these unwind through real
// frames.

class UnwindError { code: int }
class UnwindOther { weight: int }

function unwind_count(n: int) -> int {
  let x = 0;
  for (let i = 0; i < n; i += 1) { x += 1; };
  x
}

// Throws from `depth` frames down: an `UnwindError` for kind 1, a string for
// 2, an `UnwindOther` for 3, a division by zero for 4, an index out of
// bounds for 5, a native's error for 6.
function unwind_dive(depth: int, kind: int) -> int throws UnwindError | UnwindOther | string | baml.errors.InvalidArgument {
  if depth > 0 {
    return unwind_dive(depth - 1, kind) + 1;
  }
  if kind == 1 { throw UnwindError { code: 7 } }
  if kind == 2 { throw "text" }
  if kind == 3 { throw UnwindOther { weight: 3 } }
  if kind == 4 { return 10 / unwind_count(0); }
  if kind == 5 { let a = [1, 2]; return a[unwind_count(5)]; }
  if kind == 6 { return (0 - unwind_count(4)).isqrt(); }
  100
}

function unwind_local() -> int {
  let n = unwind_count(3);
  let r = {
    if n == 3 { throw UnwindError { code: 5 } }
    n
  } catch (e) {
    UnwindError => e.code * 10,
  };
  r + 1
}

function unwind_through_frames() -> int {
  unwind_dive(6, 1) catch (e) {
    UnwindError => e.code,
    _ => -1,
  }
}

function unwind_no_throw() -> int {
  unwind_dive(6, 0) catch (e) {
    _ => -1,
  }
}

function unwind_arms(kind: int) -> int {
  unwind_dive(3, kind) catch (e) {
    UnwindError => e.code,
    string => e.length(),
    UnwindOther => e.weight * 100,
    _ => -1,
  }
}
function unwind_typed_arms() -> int {
  unwind_arms(1) + unwind_arms(2) * 10 + unwind_arms(3) + unwind_arms(0)
}

function unwind_only_errors(kind: int) -> int throws UnwindOther | string | baml.errors.InvalidArgument {
  unwind_dive(2, kind) catch (e) {
    UnwindError => e.code,
  }
}
function unwind_mismatch_rethrows() -> int {
  let a = unwind_only_errors(1);
  let b = unwind_only_errors(2) catch (e) {
    string => e.length() * 10,
    _ => -1,
  };
  let c = unwind_only_errors(3) catch (e) {
    UnwindOther => e.weight * 100,
    _ => -1,
  };
  a + b + c
}

function unwind_deferring(log: int[], depth: int, kind: int) -> int throws UnwindError | UnwindOther | string | baml.errors.InvalidArgument {
  defer { log.push(1); }
  defer { log.push(2); }
  let r = unwind_dive(depth, kind);
  log.push(3);
  r
}
function unwind_digits(log: int[]) -> int {
  let n = 0;
  for (let i = 0; i < log.length(); i += 1) { n = n * 10 + log[i]; };
  n
}
function unwind_defer_on_return() -> int {
  let log = [9];
  let r = unwind_deferring(log, 2, 0) catch (e) { _ => -1 };
  unwind_digits(log) * 1000 + r
}
function unwind_defer_on_throw() -> int {
  let log = [9];
  let r = unwind_deferring(log, 2, 1) catch (e) { _ => -1 };
  unwind_digits(log) * 1000 + r
}
// A handler that takes every error does not take a panic: the defers run
// and the panic goes on.
function unwind_defer_on_panic() -> int {
  let log = [9];
  let r = unwind_deferring(log, 2, 4) catch (e) { _ => -1 };
  unwind_digits(log) * 1000 + r
}
function unwind_nested_defers() -> int {
  let log = [9];
  let r = {
    defer { log.push(4); }
    unwind_deferring(log, 1, 2)
  } catch (e) { _ => -1 };
  unwind_digits(log) * 1000 + r
}

function unwind_throwing_defer(log: int[]) -> int throws UnwindError | UnwindOther | string | baml.errors.InvalidArgument {
  defer { log.push(1); }
  defer {
    log.push(2);
    if log.length() > 1 { throw "from defer" }
  }
  unwind_dive(1, 1)
}
function unwind_throw_in_defer() -> int {
  let log = [9];
  let r = unwind_throwing_defer(log) catch (e) {
    string => e.length(),
    UnwindError => -5,
    _ => -1,
  };
  unwind_digits(log) * 1000 + r
}

function unwind_throw_in_handler() -> int {
  let r = {
    unwind_dive(2, 1) catch (e) {
      UnwindError => { if e.code == 7 { throw UnwindOther { weight: 5 } } 1 },
      _ => -1,
    }
  } catch (e) {
    UnwindOther => e.weight,
    _ => -1,
  };
  r
}

function unwind_native_caught_here() -> int {
  (0 - unwind_count(4)).isqrt() catch (e) { _ => -3 }
}
function unwind_native_caught_by_caller() -> int {
  unwind_dive(4, 6) catch (e) { _ => -3 }
}

function unwind_division_passes_local_handler() -> int {
  (10 / unwind_count(0)) catch (e) { _ => -1 }
}
function unwind_index_passes_local_handler() -> int {
  let a = [1, 2];
  a[unwind_count(2)] catch (e) { _ => -1 }
}
function unwind_catch_all_passes_panic() -> int {
  unwind_dive(3, 4) catch (e) { _ => -1 }
}

function unwind_uncaught_string() -> int { unwind_dive(3, 2) }
function unwind_uncaught_panic() -> int { unwind_dive(3, 4) }
function unwind_uncaught_index() -> int { unwind_dive(3, 5) }
function unwind_uncaught_native() -> int { unwind_dive(3, 6) }
function unwind_uncaught_after_defer() -> int {
  let log = [9];
  unwind_deferring(log, 2, 2)
}

// The callee allocates before it throws, so the string and the array here
// have moved by the time the handler reads them when every allocation
// collects.
function unwind_reference_kept() -> int {
  let text = `${unwind_count(12)} and ${unwind_count(34)}`;
  let list = [unwind_count(5), unwind_count(6)];
  unwind_dive(5, 1) catch (e) {
    UnwindError => e.code + text.length() * 10 + list[1] * 1000,
    _ => -1,
  }
}

function unwind_deep() -> int {
  unwind_dive(20000, 2) catch (e) {
    string => e.length(),
    _ => -1,
  }
}

function unwind_loop() -> int {
  let total = 0;
  for (let i = 0; i < 1000; i += 1) {
    total += unwind_dive(3, i % 4) catch (e) {
      UnwindError => e.code,
      string => 2,
      _ => 3,
    };
  };
  total
}

// A panic is an instance of its class in `baml.panics`, and an error a
// native throws one of its class in `baml.errors`: a handler names the
// class and reads the fields.
function unwind_index_fields() -> int throws UnwindError | UnwindOther | string | baml.errors.InvalidArgument {
  unwind_dive(3, 5) catch (e) {
    baml.panics.IndexOutOfBounds => e.index * 100 + e.length,
  }
}
function unwind_division_dividend() -> int throws UnwindError | UnwindOther | string | baml.errors.InvalidArgument {
  unwind_dive(3, 4) catch (e) {
    baml.panics.DivisionByZero => match (e.dividend) {
      let dividend: int => dividend,
      _ => -1,
    },
  }
}
function unwind_native_error_fields() -> int throws UnwindError | UnwindOther | string {
  unwind_dive(3, 6) catch (e) {
    baml.errors.InvalidArgument => {
      let message = e.message;
      message.length()
    },
  }
}
function unwind_local_panic_class() -> int {
  (10 / unwind_count(0)) catch (e) {
    baml.panics.DivisionByZero => 4,
  }
}

function unwind_any_panic(kind: int) -> int throws UnwindError | UnwindOther | string | baml.errors.InvalidArgument {
  unwind_dive(2, kind) catch (e) {
    baml.panics.Panic => 9,
  }
}
function unwind_panic_alias() -> int {
  let caught = unwind_any_panic(4) + unwind_any_panic(5) * 10 + unwind_any_panic(0);
  let passed = unwind_any_panic(1) catch (e) {
    UnwindError => e.code,
    _ => -1,
  };
  caught * 10 + passed
}

// A handler with no arm for a panic's class lets it by, as one that takes
// every error does.
function unwind_other_panic(kind: int) -> int throws UnwindError | UnwindOther | string | baml.errors.InvalidArgument {
  unwind_dive(2, kind) catch (e) {
    baml.panics.DivisionByZero => 1,
  }
}
function unwind_panic_class_mismatch() -> int {
  unwind_other_panic(5) catch (e) {
    baml.panics.DivisionByZero => 2,
    baml.panics.IndexOutOfBounds => 3,
    _ => -1,
  }
}

// The handlers an error passes run nothing, the defers between them all
// run, innermost first, and the first handler with an arm for it takes it.
function unwind_passing(log: int[], kind: int) -> int throws UnwindError | UnwindOther | string | baml.errors.InvalidArgument {
  defer { log.push(4); }
  let r = unwind_deferring(log, 2, kind) catch (e) {
    UnwindOther => { log.push(7); e.weight },
  };
  log.push(8);
  r
}
function unwind_passing_outer(log: int[], kind: int) -> int throws UnwindError | UnwindOther | string {
  defer { log.push(5); }
  unwind_passing(log, kind) catch (e) {
    baml.errors.InvalidArgument => { log.push(6); -6 },
  }
}
function unwind_skips(kind: int) -> int {
  let log = [9];
  let r = unwind_passing_outer(log, kind) catch (e) {
    UnwindError => e.code,
    baml.panics.Panic => -2,
    _ => -1,
  };
  unwind_digits(log) * 100 + r
}
function unwind_mismatch_skipped() -> int { unwind_skips(1) }
function unwind_match_in_the_middle() -> int { unwind_skips(3) }
function unwind_native_error_skips() -> int { unwind_skips(6) }
function unwind_panic_through_defers() -> int { unwind_skips(4) }

// Handlers one inside another in one function: what the inner one does not
// take is the outer one's, with the `defer` between them run.
function unwind_nested(log: int[], kind: int) -> int throws string {
  {
    defer { log.push(3); }
    unwind_dive(2, kind) catch (e) {
      UnwindOther => e.weight,
    }
  } catch (e) {
    UnwindError => e.code,
    baml.errors.InvalidArgument => -6,
    baml.panics.Panic => -2,
  }
}
function unwind_nested_handlers() -> int {
  let log = [9];
  let r = unwind_nested(log, 1) * 1000 + unwind_nested(log, 3) * 100 + unwind_nested(log, 4) + unwind_nested(log, 0);
  let s = unwind_nested(log, 2) catch (e) { string => e.length(), _ => -1 };
  unwind_digits(log) * 100000 + r * 10 + s
}

enum UnwindColor { Red Green }
class UnwindReport {
  cause: UnwindError
  label: string
  ratio: float
  final: bool
  color: UnwindColor
  retries: int?
  after: int?
  next: UnwindError?
}
function unwind_report() -> int throws UnwindReport {
  if unwind_count(1) == 1 {
    throw UnwindReport {
      cause: UnwindError { code: unwind_count(4) },
      label: "a \"label\"",
      ratio: 1.5,
      final: true,
      color: UnwindColor.Green,
      retries: unwind_count(2),
      after: null,
      next: null,
    }
  }
  0
}
function unwind_uncaught_class() -> int { unwind_dive(3, 1) }
function unwind_uncaught_nested_class() -> int { unwind_report() }
function unwind_uncaught_shift() -> int { 1 << (0 - unwind_count(3)) }

// An arm for an interface takes every class that implements it.
interface UnwindCoded {
  function coded(self) -> int throws never
}
class UnwindCodedA {
  a: int
  implements UnwindCoded {
    function coded(self) -> int throws never { self.a }
  }
}
class UnwindCodedB {
  b: int
  implements UnwindCoded {
    function coded(self) -> int throws never { self.b * 2 }
  }
}
function unwind_coded(kind: int) -> int throws UnwindCodedA | UnwindCodedB | string {
  if unwind_count(kind) == 1 { throw UnwindCodedA { a: 3 } }
  if unwind_count(kind) == 2 { throw UnwindCodedB { b: 4 } }
  throw "neither"
}
function unwind_coded_arm(kind: int) -> int throws string {
  unwind_coded(kind) catch (e) {
    UnwindCoded => e.coded(),
  }
}
function unwind_interface_arm() -> int {
  let passed = unwind_coded_arm(3) catch (e) { string => e.length(), _ => -1 };
  unwind_coded_arm(1) * 100 + unwind_coded_arm(2) * 10 + passed
}
