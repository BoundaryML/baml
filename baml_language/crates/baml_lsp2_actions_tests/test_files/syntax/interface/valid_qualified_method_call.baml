// BEP-044 §"Method Disambiguation": when two interfaces declare the same
// method name, `obj.InterfaceName.method()` selects the right vtable.

interface Serializer {
  function encode(self) -> string
}

interface IntSerializer {
  function encode(self) -> int
}

class Data {
  implements Serializer {
    function encode(self) -> string { return "json" }
  }
  implements IntSerializer {
    function encode(self) -> int { return 42 }
  }
}

function pick_text(d: Data) -> string {
  return d.Serializer.encode()
}

function pick_int(d: Data) -> int {
  return d.IntSerializer.encode()
}

//----
//- diagnostics
// <no-diagnostics-expected>
