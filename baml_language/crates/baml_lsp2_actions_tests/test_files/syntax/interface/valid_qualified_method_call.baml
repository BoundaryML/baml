// BEP-044 §"Method Disambiguation": when two interfaces declare the same
// method name, `obj.as<InterfaceName>.method()` selects the right vtable.

interface Serializer {
  function encode(self) -> string throws never
}

interface IntSerializer {
  function encode(self) -> int throws never
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
  return d.as<Serializer>.encode()
}

function pick_int(d: Data) -> int {
  return d.as<IntSerializer>.encode()
}

//----
//- diagnostics
// <no-diagnostics-expected>
