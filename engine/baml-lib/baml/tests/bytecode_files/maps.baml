function CreateMap() -> map<string, string> {
    { hello "world" }
}

function CreateMapJSON() -> map<string, string> {
    {"hello": "world"}
}

function UseMap() -> string {
    let map = CreateMap();
    map["hello"]
}

function UseMapNoKey() -> string {
    let map = CreateMapJSON();
    map["world"]
}

function EditMapKey() -> int {
	let map = { hi 123 };

	map["hi"] = 42 - 4;
	map["hi"] += 4;

	map["hi"]

}

function UseMapContains() -> string {
    let map = CreateMapJSON();
    if (map.has("hello")) {
        map["hello"]
    } else {
        "hi"
    }
}

function Len() -> int {
    let map = CreateMap();
    map.length()
}