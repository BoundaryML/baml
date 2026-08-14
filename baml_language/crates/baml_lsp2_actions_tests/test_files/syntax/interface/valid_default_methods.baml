// Valid: empty `implements` block inherits the interface's default method.

interface Printable {
  function display(self) -> string throws never {
    return "<printable>"
  }
}

class Item {
  implements Printable {}
}

//----
//- diagnostics
// <no-diagnostics-expected>
