// BEP-044 §"Calling the default from an override": `default.method()` inside
// an `implements` block calls the interface's own default body.

interface Logger {
  function log(self, msg: string) -> string throws never {
    return "[LOG] " + msg
  }
}

class TimestampLogger {
  prefix: string

  implements Logger {
    function log(self, msg: string) -> string {
      return self.prefix + " " + default.log(msg)
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
