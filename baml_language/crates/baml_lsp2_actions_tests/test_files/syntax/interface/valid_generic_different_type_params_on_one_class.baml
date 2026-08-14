// BEP-044: a class may implement the same generic interface with different
// type arguments. `.as<Interface<T>>` selects the instantiation at call sites.

interface Converter<T> {
  function convert(self) -> T throws never
}

class MultiFormat {
  data: string

  implements Converter<int> {
    function convert(self) -> int { return 0 }
  }

  implements Converter<float> {
    function convert(self) -> float { return 0.0 }
  }
}

function pick_int(m: MultiFormat) -> int {
  return m.as<Converter<int>>.convert()
}

function pick_float(m: MultiFormat) -> float {
  return m.as<Converter<float>>.convert()
}

//----
//- diagnostics
// <no-diagnostics-expected>
