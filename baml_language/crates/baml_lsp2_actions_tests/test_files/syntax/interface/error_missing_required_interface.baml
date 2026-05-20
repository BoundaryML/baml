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
    implements Person {
        occupation: string
    }
}

//----
//- diagnostics
// Error: class `Bad` implements `Person`, which requires `Named`, `Aged`, but `Bad` does not implement them
//     ╭─[ interface_error_missing_required_interface.baml:17:15 ]
//     │
//  17 │     implements Person {
//     │               ───┬───  
//     │                  ╰───── requires `Named`, `Aged`
//     │ 
//     │ Note: Error code: E0125
// ────╯
