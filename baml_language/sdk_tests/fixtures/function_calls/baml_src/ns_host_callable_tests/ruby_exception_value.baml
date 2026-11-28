function catch_ruby_exception(callback: () -> int throws baml.errors.HostCallable) -> baml.errors.HostCallable {
  callback() catch (e) {
    baml.errors.HostCallable => { return e; }
  };
  throw "expected callback to throw"
}

function forward_ruby_exception(value: baml.errors.HostCallable, callback: (baml.errors.HostCallable) -> int throws never) -> int {
  callback(value)
}
