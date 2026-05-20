// BEP-044 §"Generic Interfaces": implementing a concrete instantiation of a
// generic interface should compile cleanly.

interface Container<T> {
  function add(self, item: T) -> null
  function get(self, index: int) -> T?
  function size(self) -> int { return 0 }
}

class IntStack {
  items: int[] = []

  implements Container<int> {
    function add(self, item: int) -> null {
      self.items = self.items + [item]
      return null
    }
    function get(self, index: int) -> int? {
      return self.items[index]
    }
    function size(self) -> int {
      return self.items.length()
    }
  }
}

//----
//- diagnostics
// Error: Expected Unexpected token in class body, found '='
//     ╭─[ interface_valid_generic_concrete_type_param.baml:11:16 ]
//     │
//  11 │   items: int[] = []
//     │                ┬  
//     │                ╰── Expected Unexpected token in class body, found '='
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in class body, found '['
//     ╭─[ interface_valid_generic_concrete_type_param.baml:11:18 ]
//     │
//  11 │   items: int[] = []
//     │                  ┬  
//     │                  ╰── Expected Unexpected token in class body, found '['
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected Unexpected token in class body, found ']'
//     ╭─[ interface_valid_generic_concrete_type_param.baml:11:19 ]
//     │
//  11 │   items: int[] = []
//     │                   ┬  
//     │                   ╰── Expected Unexpected token in class body, found ']'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: operator `Add` cannot be applied to `int[]` and `int[]`
//     ╭─[ interface_valid_generic_concrete_type_param.baml:15:19 ]
//     │
//  15 │       self.items = self.items + [item]
//     │                   ──────────┬─────────  
//     │                             ╰─────────── operator `Add` cannot be applied to `int[]` and `int[]`
//     │ 
//     │ Note: Error code: E0004
// ────╯
