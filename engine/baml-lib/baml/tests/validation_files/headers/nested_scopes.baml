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
