class Bar {
    a int
    b string
}

function Foo() -> Bar {
  let x = Bar { a: "hello", c: 12 };
  let req_bad = baml.HttpRequest {
    url: 10,
    headers: {},
    query_params: { a 10 },
  };
  let req_good = baml.HttpRequest {
    url: "https://example.com",
    headers: { Authorization "Bearer mytoken" },
    query_params: { foo "bar" },
  };
  x
}

// error: Error validating: Bar.a expected type int, but found string
//   -->  expr/constructors_invalid.baml:7
//    | 
//  6 | function Foo() -> Bar {
//  7 |   let x = Bar { a: "hello", c: 12 };
//    | 
// error: Error validating: Class Bar has no field c
//   -->  expr/constructors_invalid.baml:7
//    | 
//  6 | function Foo() -> Bar {
//  7 |   let x = Bar { a: "hello", c: 12 };
//    | 
// error: Error validating: Class Bar is missing fields: b
//   -->  expr/constructors_invalid.baml:7
//    | 
//  6 | function Foo() -> Bar {
//  7 |   let x = Bar { a: "hello", c: 12 };
//    | 
// error: Error validating: baml.HttpRequest.url expected type string, but found int
//   -->  expr/constructors_invalid.baml:9
//    | 
//  8 |   let req_bad = baml.HttpRequest {
//  9 |     url: 10,
//    | 
// error: Error validating: baml.HttpRequest.query_params expected type (map<string, string> | null), but found map<string, int>
//   -->  expr/constructors_invalid.baml:11
//    | 
// 10 |     headers: {},
// 11 |     query_params: { a 10 },
//    | 
// error: Error validating: Class baml.HttpRequest is missing fields: method
//   -->  expr/constructors_invalid.baml:8
//    | 
//  7 |   let x = Bar { a: "hello", c: 12 };
//  8 |   let req_bad = baml.HttpRequest {
//  9 |     url: 10,
// 10 |     headers: {},
// 11 |     query_params: { a 10 },
// 12 |   };
//    | 
// error: Error validating: Class baml.HttpRequest is missing fields: method
//   -->  expr/constructors_invalid.baml:13
//    | 
// 12 |   };
// 13 |   let req_good = baml.HttpRequest {
// 14 |     url: "https://example.com",
// 15 |     headers: { Authorization "Bearer mytoken" },
// 16 |     query_params: { foo "bar" },
// 17 |   };
//    | 
