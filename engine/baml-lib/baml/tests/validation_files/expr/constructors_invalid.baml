class Bar {
    a int
    b string
}

function Foo() -> Bar {
  let x = Bar { a: "hello", c: 12 };
<<<<<<< HEAD
  let req_bad = baml.HttpRequest { 
    url: 10,
    headers: {},
    query_params: { a 10 },
  };
  let req_good = baml.HttpRequest {
    url: "https://example.com",
=======
  let req_bad = baml.Request {
    base_url: 10,
    headers: {},
    query_params: { a 10 },
  };
  let req_good = baml.Request {
    base_url: "https://example.com",
>>>>>>> canary
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
<<<<<<< HEAD
// error: Error validating: baml.HttpRequest.url expected type string, but found int
//   -->  expr/constructors_invalid.baml:9
//    | 
//  8 |   let req_bad = baml.HttpRequest { 
//  9 |     url: 10,
//    | 
// error: Error validating: baml.HttpRequest.query_params expected type (map<string, string> | null), but found map<string, int>
=======
// error: Error validating: baml.Request.base_url expected type string, but found int
//   -->  expr/constructors_invalid.baml:9
//    | 
//  8 |   let req_bad = baml.Request {
//  9 |     base_url: 10,
//    | 
// error: Error validating: baml.Request.query_params expected type map<string, string>, but found map<string, int>
>>>>>>> canary

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  expr/constructors_invalid.baml:6
//    | 
//  5 | 
//  6 | function Foo() -> Bar {
//  7 |   let x = Bar { a: "hello", c: 12 };
//    | 
