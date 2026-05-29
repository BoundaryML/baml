// Two `implements Animal` blocks on the same class — only one is allowed.

interface Animal {
  function speak(self) -> string
}

class Dog {
  implements Animal {
    function speak(self) -> string {
      return "Woof!"
    }
  }

  implements Animal {
    function speak(self) -> string {
      return "Woof again?"
    }
  }
}

//----
//- diagnostics
// error: class `Dog` has multiple `implements user.Animal` blocks; a class may implement each interface at most once
//     ╭─[ interface_duplicate_implements.baml:14:13 ]
//     │
//   8 │   implements Animal {
//     │             ───┬───  
//     │                ╰───── first `implements` block here
//     │ 
//  14 │   implements Animal {
//     │             ───┬───  
//     │                ╰───── duplicate `implements` block
//     │ 
//     │ Note: Error code: E0114
// ────╯
