// Codegen for the Python host: `baml generate` emits the typed `baml_sdk`
// package into python_demo/, next to the Python scripts that import it (so no
// sys.path tweaking). Re-run after changing any function or type that
// Python imports. NOTE: output_dir is relative to the project root (where
// baml.toml lives), not to baml_src/.
generator target {
    output_type: "python/pydantic",
    output_dir: "python_demo",
    default_client_mode: "sync",
    // the python/pydantic generator only supports preserve-case
    naming_convention: "preserve-case",
}

// Codegen for the Node/TypeScript host: emits the typed `baml_sdk` package
// into typescript_demo/, next to the TypeScript scripts that import it. The TS SDK
// always emits both sync and `_async` bindings, so there is no client-mode
// knob here. Generated code imports the `@boundaryml/baml-core-node` runtime.
// (Like the Python target, output_dir is relative to the project root and the
// `baml_sdk` package name is appended automatically.) Re-run `baml generate`
// after changing any function or type the TS side imports.
generator ts_target {
    output_type: "typescript/node",
    output_dir: "typescript_demo",
    // the typescript/node generator only supports preserve-case
    naming_convention: "preserve-case",
}
