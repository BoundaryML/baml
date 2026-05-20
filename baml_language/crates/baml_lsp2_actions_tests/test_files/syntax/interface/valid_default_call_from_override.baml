// BEP-044 §"Calling the default from an override": `default.method()` inside
// an `implements` block calls the interface's own default body.

interface Logger {
  function log(self, msg: string) -> string {
    return "[LOG] " + msg
  }
}

class TimestampLogger {
  prefix: string = "TS"

  implements Logger {
    function log(self, msg: string) -> string {
      return self.prefix + " " + default.log(msg)
    }
  }
}

//----
//- diagnostics
// Error: Expected Unexpected token in class body, found '='
//     ╭─[ interface_valid_default_call_from_override.baml:11:18 ]
//     │
//  11 │   prefix: string = "TS"
//     │                  ┬  
//     │                  ╰── Expected Unexpected token in class body, found '='
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in class body, found '"'
//     ╭─[ interface_valid_default_call_from_override.baml:11:20 ]
//     │
//  11 │   prefix: string = "TS"
//     │                    ┬  
//     │                    ╰── Expected Unexpected token in class body, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in class body, found '"'
//     ╭─[ interface_valid_default_call_from_override.baml:15:30 ]
//     │
//  15 │       return self.prefix + " " + default.log(msg)
//     │                              ┬  
//     │                              ╰── Expected Unexpected token in class body, found '"'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in class body, found '+'
//     ╭─[ interface_valid_default_call_from_override.baml:15:32 ]
//     │
//  15 │       return self.prefix + " " + default.log(msg)
//     │                                ┬  
//     │                                ╰── Expected Unexpected token in class body, found '+'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: field 'default' is missing a type annotation
//     ╭─[ interface_valid_default_call_from_override.baml:15:34 ]
//     │
//  15 │       return self.prefix + " " + default.log(msg)
//     │                                  ───┬───  
//     │                                     ╰───── field 'default' is missing a type annotation
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in class body, found '.'
//     ╭─[ interface_valid_default_call_from_override.baml:15:41 ]
//     │
//  15 │       return self.prefix + " " + default.log(msg)
//     │                                         ┬  
//     │                                         ╰── Expected Unexpected token in class body, found '.'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ interface_valid_default_call_from_override.baml:17:3 ]
//     │
//  17 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ interface_valid_default_call_from_override.baml:18:1 ]
//     │
//  18 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: unresolved type: msg
//     ╭─[ interface_valid_default_call_from_override.baml:15:45 ]
//     │
//  15 │       return self.prefix + " " + default.log(msg)
//     │                                             ──┬──  
//     │                                               ╰──── unresolved type: msg
//     │ 
//     │ Note: Error code: E0002
// ────╯
