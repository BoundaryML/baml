
// This helps use auto generate libraries you can use in the language of
// your choice. You can have multiple generators if you use multiple languages.
// Just ensure that the output_dir is different for each generator.
generator target {
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet"
    output_type "python/pydantic"
    // Where the generated code will be saved (relative to baml_src/)
    output_dir "../"
    // The version of the BAML package you have installed (e.g. same version as your baml-py or @boundaryml/baml).
    // The VSCode extension version should also match this version.
    version "0.50.0"
}
        