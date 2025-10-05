function Helper(x: int) -> int {
    x
}

function TestArray() -> int[] {
    let arr = [1, 2, 3];
    let result = Helper(42);
    [arr[0], result]
}

function ArrayAccess() -> int {
    let arr = [1, 2, 3];
    arr[1]
}

// function Helper(x) {
//   x
// }
//
// function TestArray() {
//   let arr = [1, 2, 3];
//   let result = Helper(42);
//
//   [arr[0], result]
// }
//
// function ArrayAccess() {
//   let arr = [1, 2, 3];
//
//   arr[1]
// }
//
// class baml.HttpRequest {
//   url: string
//   method: baml.HttpMethod
//   headers: (map<string, string> | null)
//   query_params: (map<string, string> | null)
//   json: (ANY | null)
// }
//
// enum baml.HttpMethod {
//   Get
//   Post
//   Put
//   Delete
//   Patch
// }
