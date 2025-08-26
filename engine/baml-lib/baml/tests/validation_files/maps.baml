fn CreateMap() -> map<string, string> {
    { hello "world" }
}

fn CreateMapJSON() -> map<string, string> {
    {"hello": "world"}
}

fn CreateMapExpr() -> map<int, string> {
    { (1 + 1): "2" }
}

fn UseMap() -> string {
    let map = CreateMap();
    map["hello"]
}

fn UseMapNoKey() -> string {
    let map = CreateMapJSON();
    map["world"]
}

fn UseMapContains() -> string {
    let map = CreateMapExpr();
    if (map.contains(1)) {
        map[1]
    } else {
        "hi"
    }
}

// wrong input type, wrong output type.
fn UseMapTypecheck() -> int {
    let map = CreateMapExpr();
    map["string"]
}

fn InconsistentStyleUse() -> int{ 
    let some_ident = "world";

    // here, 'name' is a key, but 'some_ident' is an expression.
    // This cannot be discriminated by a context-free parser, so we'll rule it out via
    // a diagnostic. We could support it anyway with context-sensitive features like typing + name resolution.
    let map = {
        name "Hello"
        some_ident "yes"
    };

    0
}