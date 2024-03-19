// generator lang_python {
//   language python
//   // This is where your non-baml source code located
//   // (relative directory where pyproject.toml, package.json, etc. lives)
//   project_root "../python"
//   // This command is used by "baml test" to run tests
//   // defined in the playground
//   test_command "infisical run -- poetry run pytest"
//   // This command is used by "baml update-client" to install
//   // dependencies to your language environment
//   install_command "poetry add baml@latest"
//   package_version_command "poetry show baml"
// }

generator lang_typescript {
  language typescript
  // This is where your non-baml source code located
  // (relative directory where pyproject.toml, package.json, etc. lives)
  project_root "../typescript"
  // This command is used by "baml test" to run tests
  // defined in the playground
  test_command "infisical run -- npm test -- "
  // This command is used by "baml update-client" to install
  // dependencies to your language environment
  install_command "npm install"
  package_version_command "npm list"
} 