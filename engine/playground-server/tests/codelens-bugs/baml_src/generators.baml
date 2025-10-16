// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet", "rest/openapi"
    output_type "rest/openapi"

    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"

    // The version of the BAML package you have installed (e.g. same version as your baml-py or @boundaryml/baml).
    // The BAML VSCode extension version should also match this version.
    version "0.206.1"

    // 'baml-cli generate' will run this after generating openapi.yaml, to generate your OpenAPI client
    // This command will be run from within $output_dir/baml_client
    //
    // Uncomment this line to tell BAML to automatically generate an OpenAPI client for you.
    //on_generate "openapi-generator generate -i openapi.yaml -g OPENAPI_CLIENT_TYPE -o ."
}
