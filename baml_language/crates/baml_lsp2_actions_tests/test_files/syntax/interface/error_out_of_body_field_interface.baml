// Out-of-body implements cannot target field-bearing interfaces.

interface Named {
    name: string
    function greet(self) -> string throws never
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
// error: interface `Named` declares fields and can only be implemented in the class body, not out-of-body
//     ╭─[ interface_error_out_of_body_field_interface.baml:12:12 ]
//     │
//  12 │ implements Named for Robot {
//     │            ──┬──
//     │              ╰──── interface `Named` declares fields and can only be implemented in the class body, not out-of-body
//     │
//     │ Note: Error code: E0126
// ────╯
