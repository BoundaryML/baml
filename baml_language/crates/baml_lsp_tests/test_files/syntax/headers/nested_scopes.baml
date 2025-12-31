function NestedScopes() -> string {
    //# Top Level
    let x = "hello";

    //## First Section
    let z = if (x == "hello") {
        //### Inside If Block
        let y = "world";
        x + " " + y
    } else {
        //### Inside Else Block
        "goodbye"
    };

    z
}

//----
//- diagnostics
// Error: Expected string, found string | void
//    ╭─[ 0:1:1 ]
//    │
//  1 │ function NestedScopes() -> string {
//    │ │ 
//    │ ╰─ Expected string, found string | void
//    │ 
//    │ Note: Error code: E0001
// ───╯
