function Helper(x: int) -> int {
    x
}

function TestArray() -> int[] {
    let arr = [1, 2, 3];
    let result = Helper(42);
    [arr, result]
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
//   [arr, result]
// }
//
// function ArrayAccess() {
//   let arr = [1, 2, 3];
//
//   arr[1]
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
