function Foo1(arg: int) -> float {
  client Foo
}

function Foo2(arg: int) -> float {
  prompt #"..."#
}

function Foo3(arg: int) -> float {
  client bar
  prompt "..."
}


function Foo4(arg: int) -> float {
  client bar
  prompt
}


function Foo5(arg: int) -> float {
  client
  prompt #"..."#
}


function Foo6(arg: int) -> float {
  client
  prompt
}

// error: Error validating Function "Foo4": This field declaration is invalid. It is either missing a name or a type.
//   -->  functions_v2/invalid2.baml:17
//    | 
// 16 |   client bar
// 17 |   prompt
//    | 
// error: Error validating Function "Foo5": This field declaration is invalid. It is either missing a name or a type.
//   -->  functions_v2/invalid2.baml:22
//    | 
// 21 | function Foo5(arg: int) -> float {
// 22 |   client
//    | 
// error: Error validating Function "Foo6": This field declaration is invalid. It is either missing a name or a type.
//   -->  functions_v2/invalid2.baml:28
//    | 
// 27 | function Foo6(arg: int) -> float {
// 28 |   client
//    | 
// error: Error validating Function "Foo6": This field declaration is invalid. It is either missing a name or a type.
//   -->  functions_v2/invalid2.baml:29
//    | 
// 28 |   client
// 29 |   prompt
//    | 
// error: Error validating: Missing `prompt` field in function. Add to the block:
// ```
// prompt #"..."#
// ```
//   -->  functions_v2/invalid2.baml:1
//    | 
//    | 
//  1 | function Foo1(arg: int) -> float {
//    | 
// error: Error validating: Missing `client` field in function. Add to the block:
// ```
// client GPT4
// ```
//   -->  functions_v2/invalid2.baml:5
//    | 
//  4 | 
//  5 | function Foo2(arg: int) -> float {
//    | 
// error: Expected a template_string value, but received string value `"..."`.
//   -->  functions_v2/invalid2.baml:11
//    | 
// 10 |   client bar
// 11 |   prompt "..."
//    | 
// error: Error validating: Missing `prompt` field in function. Add to the block:
// ```
// prompt #"..."#
// ```
//   -->  functions_v2/invalid2.baml:9
//    | 
//  8 | 
//  9 | function Foo3(arg: int) -> float {
//    | 
// error: Error validating: Missing `prompt` field in function. Add to the block:
// ```
// prompt #"..."#
// ```
//   -->  functions_v2/invalid2.baml:15
//    | 
// 14 | 
// 15 | function Foo4(arg: int) -> float {
//    | 
// error: Error validating: Missing `client` field in function. Add to the block:
// ```
// client GPT4
// ```
//   -->  functions_v2/invalid2.baml:21
//    | 
// 20 | 
// 21 | function Foo5(arg: int) -> float {
//    | 
// error: Error validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```
//   -->  functions_v2/invalid2.baml:27
//    | 
// 26 | 
// 27 | function Foo6(arg: int) -> float {
//    | 
