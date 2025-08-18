class Bar {
    a int
    b string
}

fn Foo() -> Bar {
  let x = Bar { a: "hello", c: 12 };
  let req_bad = std::Request { 
    base_url: 10,
    headers: {},
    query_params: { a 10 },
  };
  let req_good = std::Request {
    base_url: "https://example.com",
    headers: { Authorization "Bearer mytoken" },
    query_params: { foo "bar" },
  };
  x
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  expr/constructors_invalid.baml:6
//    | 
//  5 | 
//  6 | fn Foo() -> Bar {
//    | 
// error: Error validating: Bar.a expected type int, but found string
//   -->  expr/constructors_invalid.baml:7
//    | 
//  6 | fn Foo() -> Bar {
//  7 |   let x = Bar { a: "hello", c: 12 };
//    | 
// error: Error validating: Class Bar has no field c
//   -->  expr/constructors_invalid.baml:7
//    | 
//  6 | fn Foo() -> Bar {
//  7 |   let x = Bar { a: "hello", c: 12 };
//    | 
// error: Error validating: Class Bar is missing fields: b
//   -->  expr/constructors_invalid.baml:7
//    | 
//  6 | fn Foo() -> Bar {
//  7 |   let x = Bar { a: "hello", c: 12 };
//    | 
// error: Error validating: std::Request.base_url expected type string, but found int
//   -->  expr/constructors_invalid.baml:9
//    | 
//  8 |   let req_bad = std::Request { 
//  9 |     base_url: 10,
//    | 
// error: Error validating: std::Request.query_params expected type map<string, string>, but found map<string, int>
//   -->  expr/constructors_invalid.baml:11
//    | 
// 10 |     headers: {},
// 11 |     query_params: { a 10 },
//    | 
