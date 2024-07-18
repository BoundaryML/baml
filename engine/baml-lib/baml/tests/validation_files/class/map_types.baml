// Tests for validating map types

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
}

// error: Error validating: Maps may only have strings as keys
//   -->  class/map_types.baml:12
//    | 
// 11 | 
// 12 |   b1 map<int, string>
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/map_types.baml:13
//    | 
// 12 |   b1 map<int, string>
// 13 |   b2 map<float, string>
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/map_types.baml:14
//    | 
// 13 |   b2 map<float, string>
// 14 |   b3 map<MapDummy, string>
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/map_types.baml:15
//    | 
// 14 |   b3 map<MapDummy, string>
// 15 |   b4 map<string?, string>
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/map_types.baml:16
//    | 
// 15 |   b4 map<string?, string>
// 16 |   b5 map<string | int, string>
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/map_types.baml:19
//    | 
// 18 |   c1 string | map<string, string>
// 19 |   c2 string | map<int, string>
//    | 
// error: Error validating: Maps may only have strings as keys
//   -->  class/map_types.baml:20
//    | 
// 19 |   c2 string | map<int, string>
// 20 |   c3 string | map<string?, string>
//    | 
