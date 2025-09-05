fn CreateMap() -> map<string, string> {
    { hello "world" }
}

fn CreateMapJSON() -> map<string, string> {
    {"hello": "world"}
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
    let map = CreateMapJSON();
    if (map.contains("hello")) {
        map["hello"]
    } else {
        "hi"
    }
}

fn Len() -> int {
    let map = CreateMap();
    map.len()
}