// Interface methods can be referenced directly as generic functions over
// implementors, so `Interface.method(value)` is valid.

interface Describable {
  function describe(self) -> string throws never {
    return "default"
  }
}

class Thing {
  implements Describable {}
}

function bad() -> string {
  let t = Thing {}
  return Describable.describe(t)
}

//----
//- diagnostics
// <no-diagnostics-expected>
