// Tests for validating map types

class MapDummy {
  inner string
}

class MapFields {
  a1 map<string, string>
  a2 map<string, int>
  a2 map<string, MapDummy>

  b1 map<int, string>
  b2 map<float, string>
  b3 map<MapDummy, string>
}

// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types.baml:8
//    | 
//  7 | class MapFields {
//  8 |   a1 map<string, string>
//  9 |   a2 map<string, int>
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types.baml:9
//    | 
//  8 |   a1 map<string, string>
//  9 |   a2 map<string, int>
// 10 |   a2 map<string, MapDummy>
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types.baml:10
//    | 
//  9 |   a2 map<string, int>
// 10 |   a2 map<string, MapDummy>
// 11 | 
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types.baml:12
//    | 
// 11 | 
// 12 |   b1 map<int, string>
// 13 |   b2 map<float, string>
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types.baml:13
//    | 
// 12 |   b1 map<int, string>
// 13 |   b2 map<float, string>
// 14 |   b3 map<MapDummy, string>
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  class/map_types.baml:14
//    | 
// 13 |   b2 map<float, string>
// 14 |   b3 map<MapDummy, string>
// 15 | }
//    | 
