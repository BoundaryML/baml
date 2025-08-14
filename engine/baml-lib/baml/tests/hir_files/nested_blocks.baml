function Foo() -> int {
    let x = {
        let y = 1;
        y
    };
    x
}

// function Foo() {
//   let x = {
//   let y = 1;
//     y
//   };
//   return x;
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