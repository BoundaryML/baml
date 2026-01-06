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

//----
//- diagnostics
// Error: Unknown property 'language' in generator 'default'. Valid properties: output_type, output_dir, version, default_client_mode, on_generate, project, client_package_name, module_format
//    ╭─[ generators_error.baml:2:3 ]
//    │
//  2 │   language python
//    │   ────┬───  
//    │       ╰───── Unknown property 'language' in generator 'default'. Valid properties: output_type, output_dir, version, default_client_mode, on_generate, project, client_package_name, module_format
//    │ 
//    │ Note: Error code: E0017
// ───╯
// Error: Unknown property 'o' in generator 'default'. Valid properties: output_type, output_dir, version, default_client_mode, on_generate, project, client_package_name, module_format
//    ╭─[ generators_error.baml:3:3 ]
//    │
//  3 │   o o
//    │   ┬  
//    │   ╰── Unknown property 'o' in generator 'default'. Valid properties: output_type, output_dir, version, default_client_mode, on_generate, project, client_package_name, module_format
//    │ 
//    │ Note: Error code: E0017
// ───╯
// Error: Generator 'default' is missing required property 'output_type'
//    ╭─[ generators_error.baml:1:11 ]
//    │
//  1 │ generator default {
//    │           ───┬───  
//    │              ╰───── Generator 'default' is missing required property 'output_type'
//    │ 
//    │ Note: Error code: E0018
// ───╯
