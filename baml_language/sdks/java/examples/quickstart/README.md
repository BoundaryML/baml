# BAML Java quickstart

Minimal consumer of the `com.boundaryml:baml-bridge` Maven artifact.

```sh
# 1. Generate the typed SDK from baml_src/ (writes ./baml_sdk/)
baml generate

# 2. Build & run — resolves baml-bridge from mavenLocal()/Central and
#    loads the engine from the embedded native jar (no env vars).
gradle run -PbamlVersion=<published version>
```

Expected output:

```
add(2, 3) = 5
Hello, Maven!
```

The Gradle wiring is three ideas: `mavenLocal()`/`mavenCentral()` +
the `baml-bridge` dependency (plus its `natives-<platform>` classifier
jar), the generated tree registered as a source root (parent of
`baml_sdk/`, resources include the `.b64` bytecode), and
`options.release = 17`.
