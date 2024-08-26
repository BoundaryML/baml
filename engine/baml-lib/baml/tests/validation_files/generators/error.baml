generator default {
  language python
  o o
}

// error: Property not known: "language". Did you mean one of these: "version", "output_type", "output_dir", "default_client_mode"?
//   -->  generators/error.baml:2
//    | 
//  1 | generator default {
//  2 |   language python
//    | 
// error: Property not known: "o". Did you mean one of these: "version", "output_dir", "output_type", "default_client_mode"?
//   -->  generators/error.baml:3
//    | 
//  2 |   language python
//  3 |   o o
//    | 
