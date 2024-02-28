generator default {
  // Right now only python is supported, but soon TS
  language python

  // Uncomment this line if you use poetry
  pkg_manager poetry
}


// warning: The generator format is deprecated. Please use the new format.
// generator default {
//     language "python"
//     project_root "generators/../"
//     test_command "poetry run python -m pytest"
//     install_command "poetry add baml@latest"
//     package_version_command "poetry show baml"
// }
//   -->  generators/v0.baml:1
//    | 
//    | 
//  1 | generator default {
//    | 
