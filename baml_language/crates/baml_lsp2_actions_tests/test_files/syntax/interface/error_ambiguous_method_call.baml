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
// E0121
//
//   × method `encode` on class `Hybrid` is declared by multiple interfaces: `Serializer`, `BinarySerializer`; unqualified calls will be ambiguous — use obj.as<Serializer>.encode() or
//   │ obj.as<BinarySerializer>.encode()
//     ╭─[interface_error_ambiguous_method_call.baml:22:10]
//  22 │   return h.encode()
//     ·          ────────
//     ╰────
