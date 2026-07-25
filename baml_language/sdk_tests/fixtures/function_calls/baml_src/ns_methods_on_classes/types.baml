// Static + instance method coverage for host-language codegen.
//
// Unlike `ns_raises_test`'s `DocLoader` — whose methods exist only to pin
// throws-contract docstrings and always `throw` (so they are never called) —
// `Greeter`'s methods have non-throwing bodies, so the host SDK can actually
// round-trip the call through the engine: a static factory (`create`, no
// `self`) plus instance methods (`who` / `greet`, both taking `self`).

class Greeter {
  name string

  // Instance method, no extra args: returns a field off `self`.
  function who(self) -> string {
    self.name
  }

  // Instance method with a non-`self` argument: echoes the argument back.
  function greet(self, greeting: string) -> string {
    greeting
  }

  function wait(self, callback: () -> string throws never) -> string {
    callback()
  }

  // Static method (no `self`): a factory that constructs the class.
  function create(name: string) -> Greeter {
    Greeter { name: name }
  }
}
