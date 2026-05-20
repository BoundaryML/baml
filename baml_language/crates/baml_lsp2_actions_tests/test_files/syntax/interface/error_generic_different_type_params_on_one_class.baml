// A class may not implement the same interface twice — even with different
// type arguments, the method names collide and codegen has no way to
// disambiguate the resulting slot collisions.

interface Converter<T> {
  function convert(self) -> T
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

//----
//- diagnostics
// <no-diagnostics-expected>
