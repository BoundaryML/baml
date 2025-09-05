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

fn EditMapKey() -> int {
	let map = { hi 123 };

	map["hi"] = 42 - 4;
	map["hi"] += 4;

	map["hi"]

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

// function CreateMap() {
//   { "hello": "world" }
// }
//
// function CreateMapJSON() {
//   { "hello": "world" }
// }
//
// function UseMap() {
//   let map = CreateMap();
//
//   map["hello"]
// }
//
// function UseMapNoKey() {
//   let map = CreateMapJSON();
//
//   map["world"]
// }
//
// function EditMapKey() {
//   let map = { "hi": 123 };
//   map["hi"] = 42 - 4;
//   map["hi"] += 4;
//
//   map["hi"]
// }
//
// function UseMapContains() {
//   let map = CreateMapJSON();
//   if map.contains("hello") {
//   
//     map["hello"]
//   } else {
//   
//     "hi"
//   } 
// }
//
// function Len() {
//   let map = CreateMap();
//
//   map.len()
// }
//
// class std::Request {
//   base_url: string
//   headers: map<string, string>
//   query_params: map<string, string>
// }
//
// enum std::HttpMethod {
//   Get
// }
