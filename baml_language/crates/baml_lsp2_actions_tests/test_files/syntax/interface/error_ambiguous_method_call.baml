// BEP-044: when multiple interfaces contribute the same method name, a
// concrete receiver must project with `.as<Interface>` before calling.

interface Serializer {
  function encode(self) -> string throws never
}

interface BinarySerializer {
  function encode(self) -> string throws never
}

class Hybrid {
  implements Serializer {
    function encode(self) -> string { return "json" }
  }
  implements BinarySerializer {
    function encode(self) -> string { return "binary" }
  }
}

function bad(h: Hybrid) -> string {
  return h.encode()
}

//----
//- diagnostics
// <no-diagnostics-expected>
