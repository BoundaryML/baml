// Tests for validating map types

client<llm> MyClient {
  provider openai
}

class MapDummy {
  inner string
}

class MapFields {
  a1 map<string, string>
  a2 map<string, int>
  a3 map<string, MapDummy>

  b1 map<int, string>
  b2 map<float, string>
  b3 map<MapDummy, string>
  b4 map<string?, string>
  b5 map<string | int, string>

  c1 string | map<string, string>
  c2 string | map<int, string>
  c3 string | map<string?, string>

  d1 map<>
  d2 map<string>
  d3 map<string, string, string>
}

function InputAndOutput(i1: map<string, string>, i2: map<MapDummy, string>) -> map<string, int> {
  client MyClient 
  prompt #"
    Merge these: {{i1}} {{i2}}
  "#
}

// error: Error validating function "MyClient": Invalid syntax: missing input parameters and return type. Valid function syntax is
// ```
// function MyClient(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  class/map_types2.baml:3
//    | 
//  2 | 
//  3 | client<llm> MyClient {
//  4 |   provider openai
//  5 | }
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types2.baml:26
//    | 
// 25 | 
// 26 |   d1 map<>
// 27 |   d2 map<string>
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types2.baml:27
//    | 
// 26 |   d1 map<>
// 27 |   d2 map<string>
// 28 |   d3 map<string, string, string>
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types2.baml:28
//    | 
// 27 |   d2 map<string>
// 28 |   d3 map<string, string, string>
// 29 | }
//    | 
