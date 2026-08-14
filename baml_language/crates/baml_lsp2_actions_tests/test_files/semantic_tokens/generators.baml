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

//----
//- diagnostics
// E0017
//
//   ⚠ `generator target` is ignored: code generators are configured in `baml.toml` now. Move it to a `[generator.<name>]` section in `baml.toml`.
//    ╭─[generators.baml:6:1]
//  6 │ generator target {
//    · ────────┬───────
//    ·         ╰── move this to baml.toml
//    ╰────
// E0017
//
//   ⚠ `generator ts_target` is ignored: code generators are configured in `baml.toml` now. Move it to a `[generator.<name>]` section in `baml.toml`.
//     ╭─[generators.baml:21:1]
//  21 │ generator ts_target {
//     · ─────────┬─────────
//     ·          ╰── move this to baml.toml
//     ╰────
//
//- semantic_tokens
// generators.baml:1:1 (comment) len=74 "// Codegen for the Python host: `baml generate` emits the typed `baml_sdk`"
// generators.baml:2:1 (comment) len=78 "// package into python_demo/, next to the Python scripts that import it (so no"
// generators.baml:3:1 (comment) len=70 "// sys.path tweaking). Re-run after changing any function or type that"
// generators.baml:4:1 (comment) len=74 "// Python imports. NOTE: output_dir is relative to the project root (where"
// generators.baml:5:1 (comment) len=38 "// baml.toml lives), not to baml_src/."
// generators.baml:6:1 (keyword) len=9 "generator"
// generators.baml:6:11 (struct) [declaration] len=6 "target"
// generators.baml:7:5 (property) len=11 "output_type"
// generators.baml:7:18 (string) len=1 "\""
// generators.baml:7:19 (string) len=6 "python"
// generators.baml:7:25 (string) len=1 "/"
// generators.baml:7:26 (string) len=8 "pydantic"
// generators.baml:7:34 (string) len=1 "\""
// generators.baml:8:5 (property) len=10 "output_dir"
// generators.baml:8:17 (string) len=1 "\""
// generators.baml:8:18 (string) len=11 "python_demo"
// generators.baml:8:29 (string) len=1 "\""
// generators.baml:9:5 (property) len=19 "default_client_mode"
// generators.baml:9:26 (string) len=1 "\""
// generators.baml:9:27 (string) len=4 "sync"
// generators.baml:9:31 (string) len=1 "\""
// generators.baml:10:5 (comment) len=60 "// the python/pydantic generator only supports preserve-case"
// generators.baml:11:5 (property) len=17 "naming_convention"
// generators.baml:11:24 (string) len=1 "\""
// generators.baml:11:25 (string) len=13 "preserve-case"
// generators.baml:11:38 (string) len=1 "\""
// generators.baml:14:1 (comment) len=75 "// Codegen for the Node/TypeScript host: emits the typed `baml_sdk` package"
// generators.baml:15:1 (comment) len=83 "// into typescript_demo/, next to the TypeScript scripts that import it. The TS SDK"
// generators.baml:16:1 (comment) len=75 "// always emits both sync and `_async` bindings, so there is no client-mode"
// generators.baml:17:1 (comment) len=78 "// knob here. Generated code imports the `@boundaryml/baml-core-node` runtime."
// generators.baml:18:1 (comment) len=78 "// (Like the Python target, output_dir is relative to the project root and the"
// generators.baml:19:1 (comment) len=77 "// `baml_sdk` package name is appended automatically.) Re-run `baml generate`"
// generators.baml:20:1 (comment) len=59 "// after changing any function or type the TS side imports."
// generators.baml:21:1 (keyword) len=9 "generator"
// generators.baml:21:11 (struct) [declaration] len=9 "ts_target"
// generators.baml:22:5 (property) len=11 "output_type"
// generators.baml:22:18 (string) len=1 "\""
// generators.baml:22:19 (string) len=10 "typescript"
// generators.baml:22:29 (string) len=1 "/"
// generators.baml:22:30 (string) len=4 "node"
// generators.baml:22:34 (string) len=1 "\""
// generators.baml:23:5 (property) len=10 "output_dir"
// generators.baml:23:17 (string) len=1 "\""
// generators.baml:23:18 (string) len=15 "typescript_demo"
// generators.baml:23:33 (string) len=1 "\""
// generators.baml:24:5 (comment) len=60 "// the typescript/node generator only supports preserve-case"
// generators.baml:25:5 (property) len=17 "naming_convention"
// generators.baml:25:24 (string) len=1 "\""
// generators.baml:25:25 (string) len=13 "preserve-case"
// generators.baml:25:38 (string) len=1 "\""
