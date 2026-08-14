# Generic/nullability compile evidence

Run the complete fail-closed matrix from the repository root:

```shell
bash \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.GenericCompileProbe/verify.sh
```

The executor gives the positive case and every negative case separate
`--artifacts-path` directories, restores and builds each case warning-free,
runs the positive fixture, and requires each negative build to fail with
exactly its assigned compiler code and no other error. It also proves that a
misspelled/unknown case is rejected with `BAMLGEN001`. Set
`BAML_GENERIC_PROBE_ARTIFACTS` to retain the logs at a chosen path; otherwise,
the executor uses a new temporary directory and prints its path.

The default warning-as-error build is the positive generated-style API matrix.
`BamlNegativeCase` accepts only one of the checked-in `Negative*.cs`
basenames, each of which compiles exactly one intentional failure:

- wrapper-nested raw optional/nullable inference;
- the forbidden two-user-conversion composed raw value;
- bare-null and result-only generic inference;
- nonnullable explicit null under nullable warnings-as-errors;
- raw union inference without an explicit occurrence/arm type.

Runtime binder rejection of noncanonical numerics, concrete mutable collection
closures, redundant nullable wrappers, context-free unions, unsupported map
keys, and convenience CLR types is covered by the sibling managed-contract
probe.
