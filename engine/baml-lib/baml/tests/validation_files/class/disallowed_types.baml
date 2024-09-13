class DisallowedTypes {
  p string[]?
  q map<string, string>?
}

// error: Error validating: Lists are not allowed to be optional
//   -->  class/disallowed_types.baml:2
//    | 
//  1 | class DisallowedTypes {
//  2 |   p string[]?
//    | 
// error: Error validating: Maps are not allowed to be optional
//   -->  class/disallowed_types.baml:3
//    | 
//  2 |   p string[]?
//  3 |   q map<string, string>?
//    | 
