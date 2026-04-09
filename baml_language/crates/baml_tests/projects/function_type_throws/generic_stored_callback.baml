// === Generic stored callback field ===
// The generic parameter E should flow through to the stored callback's throws.

class Handler<E> {
  run: () -> null throws E
}

function use_string_handler(h: Handler<string>) -> null {
  h.run()
}

function use_int_handler(h: Handler<int>) -> null {
  h.run()
}

function use_string_handler_alias(h: Handler<string>) -> null {
  let alias = h
  alias.run()
}

function use_local_string_handler() -> null {
  let h = make_string_handler()
  h.run()
}

function maybe_use_string_handler(h: Handler<string>?) -> null {
  h?.run()
  null
}

function make_string_handler() -> Handler<string> {
  Handler { run: () -> null { throw "error" } }
}

function make_int_handler() -> Handler<int> {
  Handler { run: () -> null { throw 42 } }
}

// Calling use_string_handler should have: throws string
function test_string_handler() -> null {
  use_string_handler(make_string_handler())
}

// Calling use_int_handler should have: throws int
function test_int_handler() -> null {
  use_int_handler(make_int_handler())
}

function test_string_handler_alias() -> null {
  use_string_handler_alias(make_string_handler())
}

function test_local_string_handler() -> null {
  use_local_string_handler()
}

function test_optional_string_handler() -> null {
  maybe_use_string_handler(make_string_handler())
}
