
generator lang_typescript {
  language typescript
  // This is where your non-baml source code located
  // (relative directory where pyproject.toml, package.json, etc. lives)
  project_root "../typescript"
  // This command is used by "baml test" to run tests
  // defined in the playground
  //test_command "infisical run -- npm test -- "
  test_command "pnpm test -- "
  // This command is used by "baml update-client" to install
  // dependencies to your language environment
  install_command "pnpm install @boundaryml/baml-core@next"
  package_version_command "pnpm list @boundaryml/baml-core"
}

generator lang_python {
  language python
  // This is where your non-baml source code located
  // (relative directory where pyproject.toml, package.json, etc. lives)
  project_root "../python"
  // This command is used by "baml test" to run tests
  // defined in the playground
  test_command "poetry run pytest -s"
  // This command is used by "baml update-client" to install
  // dependencies to your language environment
  install_command "poetry add baml@latest"
  package_version_command "poetry show baml"
}

generator lang_ruby {
  language ruby
  // This is where your non-baml source code located
  // (relative directory where pyproject.toml, package.json, etc. lives)
  project_root "../ruby"
  // This command is used by "baml test" to run tests
  // defined in the playground
  test_command "ruby test"
  // This command is used by "baml update-client" to install
  // dependencies to your language environment
  install_command "gem update baml"
  package_version_command "bundle show baml"
}
