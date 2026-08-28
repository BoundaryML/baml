# Compiler behavior tests

This namespace contains executable BAML tests for compiler behavior. Shared
assertions in `assertions.baml` use `reflect.Package._compile` to admit or
reject isolated source snippets without linking or initializing them. Diagnostic
expectations implement `DiagnosticExpectation`, and `AssertMultiRejected`
checks ordered groups of diagnostics.

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
