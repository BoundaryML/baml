fn CreateMap() -> map<string, string> {
    { hello "world" }
}

fn CreateMapJSON() -> map<string, string> {
    {"hello": "world"}
}

// For now, errors with 'map keys must be string literals'
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

fn Len() -> int {
    let map = CreateMap();
    map.len()
}