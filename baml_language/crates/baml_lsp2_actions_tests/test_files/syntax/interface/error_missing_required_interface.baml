// E0125: class implements Person which requires Named and Aged,
// but does not implement Named or Aged.

interface Named {
    name: string
}

interface Aged {
    age: int
}

interface Person requires Named, Aged {
    occupation: string
}

class Bad {
    occupation: string

    implements Person {
    }
}

//----
//- diagnostics
// error: class `Bad` implements `user.Person`, which requires `user.Named`, `user.Aged`, but `Bad` does not implement them
//     ╭─[ interface_error_missing_required_interface.baml:19:15 ]
//     │
//  19 │     implements Person {
//     │               ───┬───  
//     │                  ╰───── requires `user.Named`, `user.Aged`
//     │ 
//     │ Note: Error code: E0125
// ────╯
