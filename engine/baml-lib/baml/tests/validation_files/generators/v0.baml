generator default {
  // Right now only python is supported, but soon TS
  language python

  // Uncomment this line if you use poetry
  pkg_manager poetry
}

// warning: This generator format is deprecated. Please use the new format.
// generator default {
//     output_type "python/pydantic"
//     output_dir "../"
// }
//   -->  generators/v0.baml:1
//    | 
//    | 
//  1 | generator default {
//  2 |   // Right now only python is supported, but soon TS
//  3 |   language python
//  4 | 
//  5 |   // Uncomment this line if you use poetry
//  6 |   pkg_manager poetry
//  7 | }
//    | 
