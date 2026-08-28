# Compiler behavior tests

This namespace contains executable BAML tests for compiler behavior. Tests use
`reflect.Package.compile` when they need to admit or reject isolated source
snippets without making the outer `baml_src` project invalid.

Organize suites by compiler concept and then behavior:

```text
ns_compiler/
  ns_class_constructors/
    ns_required_fields/
  ns_generics/
    ns_inference/
  ns_diagnostics/
    ns_spans/
```

That layout produces readable canonical test IDs such as
`root.compiler.class_constructors.required_fields::<test name>`.

Do not name namespaces, helpers, or tests after issue IDs. Put issue references
in focused comments, repro documentation, commit messages, or pull requests.
