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
// Error: class `Bad` implements `Person`, which requires `Named`, `Aged`, but `Bad` does not implement them
//     ╭─[ interface_error_missing_required_interface.baml:19:15 ]
//     │
//  19 │     implements Person {
//     │               ───┬───  
//     │                  ╰───── requires `Named`, `Aged`
//     │ 
//     │ Note: Error code: E0125
// ────╯
