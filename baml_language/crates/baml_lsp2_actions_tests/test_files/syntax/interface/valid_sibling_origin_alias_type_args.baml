type Texty = string | int

interface Parent<T> {
  function describe(self) -> string
}

interface Child requires Parent<int | string> {}

class Box {
  implements Parent<Texty> {
    function describe(self) -> string {
      return "box"
    }
  }

  implements Child {}
}

//----
//- diagnostics
// <no-diagnostics-expected>
