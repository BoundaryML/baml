generator default {
  language python
  o o
}

// error: Property not known: "language". Did you mean one of these: "version", "on_generate", "project", "output_type", "output_dir", "module_format", "client_package_name", "default_client_mode"?
//   -->  generators/error.baml:2
//    | 
//  1 | generator default {
//  2 |   language python
//    | 
// error: Property not known: "o". Did you mean one of these: "version", "project", "output_dir", "output_type", "on_generate", "module_format", "default_client_mode", "client_package_name"?
//   -->  generators/error.baml:3
//    | 
//  2 |   language python
//  3 |   o o
//    | 
