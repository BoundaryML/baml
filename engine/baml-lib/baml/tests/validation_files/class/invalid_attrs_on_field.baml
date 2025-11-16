class SkipField {
    dont_skip string
    skip_this_one string @skip
}

// error: Error validating: Class field with @skip attribute must be optional. Try making the type nullable: skip_this_one string?
//   -->  class/invalid_attrs_on_field.baml:3
//    | 
//  2 |     dont_skip string
//  3 |     skip_this_one string @skip
//  4 | }
//    | 
