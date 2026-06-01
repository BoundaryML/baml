// Out-of-body implements cannot target field-bearing interfaces.

interface Named {
    name: string
    function greet(self) -> string
}

class Robot {
    model: string
}

implements Named for Robot {
    function greet(self) -> string {
        return "I am a robot"
    }
}

//----
//- diagnostics
// error: `implements Named for Robot` cannot implement field-bearing interface `Named` outside the class body
//     ╭─[ interface_error_out_of_body_field_interface.baml:12:11 ]
//     │
//  12 │ implements Named for Robot {
//     │           ───┬──  
//     │              ╰──── field-bearing interfaces must be implemented inside the class body
//     │ 
//     │ Note: Error code: E0126
// ────╯
