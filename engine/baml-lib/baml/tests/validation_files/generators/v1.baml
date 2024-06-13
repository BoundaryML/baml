generator lang_python_1 {
  language python
  // This is where your non-baml source code located
  // (relative directory where pyproject.toml, package.json, etc. lives)
  project_root "../"
  // This command is used by "baml test" to run tests
  // defined in the playground
  test_command ". ./venv/bin/activate && python -m pytest"
  // This command is used by "baml update-client" to install
  // dependencies to your language environment
  install_command ". ./venv/bin/activate && pip install --upgrade baml"
  package_version_command ". ./venv/bin/activate && pip show baml"
}

generator lang_python_0 {
  language python
  // This is where your non-baml source code located
  // (relative directory where pyproject.toml, package.json, etc. lives)
  project_root "../"
  // This command is used by "baml test" to run tests
  // defined in the playground
  test_command "poetry run pytest"
  // This command is used by "baml update-client" to install
  // dependencies to your language environment
  install_command "poetry add baml@latest"
  package_version_command "poetry show baml"
}

// warning: This generator format is deprecated. Please use the new format.
// 
// generator lang_python_1 {
//     output_type "python/pydantic"
//     output_dir "../"
// }
//   -->  generators/v1.baml:1
//    | 
//    | 
//  1 | generator lang_python_1 {
//  2 |   language python
//  3 |   // This is where your non-baml source code located
//  4 |   // (relative directory where pyproject.toml, package.json, etc. lives)
//  5 |   project_root "../"
//  6 |   // This command is used by "baml test" to run tests
//  7 |   // defined in the playground
//  8 |   test_command ". ./venv/bin/activate && python -m pytest"
//  9 |   // This command is used by "baml update-client" to install
// 10 |   // dependencies to your language environment
// 11 |   install_command ". ./venv/bin/activate && pip install --upgrade baml"
// 12 |   package_version_command ". ./venv/bin/activate && pip show baml"
// 13 | }
//    | 
// warning: This generator format is deprecated. Please use the new format.
// 
// generator lang_python_0 {
//     output_type "python/pydantic"
//     output_dir "../"
// }
//   -->  generators/v1.baml:15
//    | 
// 14 | 
// 15 | generator lang_python_0 {
// 16 |   language python
// 17 |   // This is where your non-baml source code located
// 18 |   // (relative directory where pyproject.toml, package.json, etc. lives)
// 19 |   project_root "../"
// 20 |   // This command is used by "baml test" to run tests
// 21 |   // defined in the playground
// 22 |   test_command "poetry run pytest"
// 23 |   // This command is used by "baml update-client" to install
// 24 |   // dependencies to your language environment
// 25 |   install_command "poetry add baml@latest"
// 26 |   package_version_command "poetry show baml"
// 27 | }
//    | 
