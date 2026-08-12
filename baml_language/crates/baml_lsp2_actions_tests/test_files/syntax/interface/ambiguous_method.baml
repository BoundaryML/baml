// Two `implements` blocks declare the same method name — unqualified calls
// would be ambiguous. BEP-044 §"Method Disambiguation".

interface Serializer {
  function encode(self) -> string throws never
}

interface BinarySerializer {
  function encode(self) -> string throws never
}

class Hybrid {
  implements Serializer {
    function encode(self) -> string {
      return "json"
    }
  }
  implements BinarySerializer {
    function encode(self) -> string {
      return "bin"
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
