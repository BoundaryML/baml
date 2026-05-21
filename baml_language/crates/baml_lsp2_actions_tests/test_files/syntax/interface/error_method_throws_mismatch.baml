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
// error: method `run` in class `Worker` does not match interface `Fallible`: expected `() -> string throws IoError`, found `() -> string`
//     ╭─[ interface_error_method_throws_mismatch.baml:11:14 ]
//     │
//  11 │     function run(self) -> string {
//     │              ─┬─  
//     │               ╰─── signature mismatch
//     │ 
//     │ Note: Error code: E0120
// ────╯
