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
<<<<<<< HEAD
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
=======
>>>>>>> canary

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  test.baml:32
//    | 
// 31 | // }
// 32 | <<<<<<< HEAD
// 33 | //
//    | 
