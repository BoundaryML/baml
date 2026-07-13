// BEP-044: generic bounds may target a concrete generic interface
// instantiation, and `.as<Interface<T>>` selects that same view.

interface Converter<T> {
  function convert(self) -> T throws never
}

class MultiFormat {
  data: string

  implements Converter<int> {
    function convert(self) -> int { return 42 }
  }
  implements Converter<float> {
    function convert(self) -> float { return 42.5 }
  }
}

function read_int<T extends Converter<int>>(m: T) -> int {
  return m.as<Converter<int>>.convert()
}

//----
//- diagnostics
// <no-diagnostics-expected>
