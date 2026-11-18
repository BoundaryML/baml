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
