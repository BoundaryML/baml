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

// String: "world"
// String: "hello"
// Function: CreateMap
// 1   0   LOAD_CONST 0   (world)
//     1   LOAD_CONST 1   (hello)
//     2   ALLOC_MAP 1    
//     3   RETURN         
//
// String: "world"
// String: "hello"
// Function: CreateMapJSON
// 5   0   LOAD_CONST 0   (world)
//     1   LOAD_CONST 1   (hello)
//     2   ALLOC_MAP 1    
//     3   RETURN         
//
// String: "hello"
// Function: UseMap
// 9    0   LOAD_GLOBAL 0      (<fn CreateMap>)
//      1   CALL 0             
//
// 10   2   LOAD_VAR 1         (map)
//      3   LOAD_CONST 0       (hello)
//      4   LOAD_MAP_ELEMENT   
//      5   RETURN             
//
// String: "world"
// Function: UseMapNoKey
// 14   0   LOAD_GLOBAL 1      (<fn CreateMapJSON>)
//      1   CALL 0             
//
// 15   2   LOAD_VAR 1         (map)
//      3   LOAD_CONST 0       (world)
//      4   LOAD_MAP_ELEMENT   
//      5   RETURN             
//
// String: "hi"
// String: "hi"
// String: "hi"
// String: "hi"
// Function: EditMapKey
// 19   0    LOAD_CONST 0        (123)
//      1    LOAD_CONST 1        (hi)
//      2    ALLOC_MAP 1         
//
// 21   3    LOAD_VAR 1          (map)
//      4    LOAD_CONST 2        (hi)
//      5    LOAD_CONST 3        (42)
//      6    LOAD_CONST 4        (4)
//      7    BIN_OP -            
//      8    STORE_MAP_ELEMENT   
//
// 22   9    LOAD_VAR 1          (map)
//      10   LOAD_CONST 5        (hi)
//      11   COPY 1              
//      12   COPY 1              
//      13   LOAD_MAP_ELEMENT    
//      14   LOAD_CONST 6        (4)
//      15   BIN_OP +            
//      16   STORE_MAP_ELEMENT   
//
// 24   17   LOAD_VAR 1          (map)
//      18   LOAD_CONST 7        (hi)
//      19   LOAD_MAP_ELEMENT    
//      20   RETURN              
//
// String: "hello"
// String: "hello"
// String: "hi"
// Function: UseMapContains
// 29   0    LOAD_GLOBAL 1      (<fn CreateMapJSON>)
//      1    CALL 0             
//
// 30   2    LOAD_GLOBAL 11     (<fn std.Map.contains>)
//      3    LOAD_VAR 1         (map)
//      4    LOAD_CONST 0       (hello)
//      5    CALL 2             
//      6    JUMP_IF_FALSE +6   (to 12)
//      7    POP 1              
//
// 31   8    LOAD_VAR 1         (map)
//      9    LOAD_CONST 1       (hello)
//      10   LOAD_MAP_ELEMENT   
//      11   JUMP +3            (to 14)
//      12   POP 1              
//
// 33   13   LOAD_CONST 2       (hi)
//      14   RETURN             
//
// Function: Len
// 38   0   LOAD_GLOBAL 0    (<fn CreateMap>)
//      1   CALL 0           
//
// 39   2   LOAD_GLOBAL 10   (<fn std.Map.len>)
//      3   LOAD_VAR 1       (map)
//      4   CALL 1           
//      5   RETURN           
//
// Class: std::Request with 3 fields
// Enum std::HttpMethod
// Function: std.Array.len
//
// Function: std.Map.len
//
// Function: std.Map.contains
