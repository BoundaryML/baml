class User {
    name string
    age int

    function Greet(self) -> string {
        "Hello"
    }

    function IsAdult(self) -> bool {
        self.age >= 18
    }

    // TODO: left side of assignment is not an identifier: FieldAccess
    // function SetName(self, name: string) -> bool {
    //     self.name = name;
    //     true
    // }
}

//----
//- diagnostics
// Error: Expected string, found void
//    ╭─[ 0:1:1 ]
//    │
//  1 │ class User {
//    │ │ 
//    │ ╰─ Expected string, found void
//    │ 
//    │ Note: Error code: E0001
// ───╯
