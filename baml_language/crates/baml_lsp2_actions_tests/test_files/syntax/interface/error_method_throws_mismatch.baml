class IoError {
  message: string
}

interface Fallible {
  function run(self) -> string throws IoError
}

class Worker {
  implements Fallible {
    function run(self) -> string {
      return "ok"
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
