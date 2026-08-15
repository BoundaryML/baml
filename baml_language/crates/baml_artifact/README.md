# BAML package artifacts

A package artifact is the compiler boundary between a package producer and its
dependents. It is not a cache entry and it is not stdlib-specific.

## Container

The file starts with a magic value, format version, and Borsh-encoded header.
The header identifies the exact compiler source build, bytecode ABI, package,
source hash, interface hash, dependency interface hashes, and section table.
Each section has an independent SHA-256 digest and is validated only when read.

Sections are:

- `Interface`: the semantic ABI required to check dependents.
- `Code`: relocatable compilation units required to link an executable.
- `Tooling`: optional documentation and editor metadata.
- `Sources`: optional source text for debugging and source distribution.

`baml check` must only decode `Interface`. Linking must only add `Code`. Neither
operation may instantiate dependency `SourceFile`s.

## Semantic ABI

`PackageInterface` owns the semantic data needed by dependent type checking:

- classes, enums, aliases, and interfaces;
- free, static, instance, default, and builtin function signatures;
- generic bounds, associated types, and defaults;
- interface implementation and coherence facts;
- class and interface field layout;

Resolvers return source-backed or compiled symbols over the same semantic
operations. A database installs compiled interfaces before setting its project
root, so files belonging to compiled packages are never instantiated.

Recursive aliases remain nominal in the interface and expand through the
ordinary type-fact oracle. Compiler language items are identified by qualified
package names, not source locations.

## Code and linking

`PackageCode` owns symbolic `CompilationUnit`s. Imports identify a package plus
symbol, and exports identify package-local definitions. The loader validates
dependency interface hashes, topologically sorts the package graph, and passes
one explicit linker group per package. The linker assigns final object/global
indexes and appends each package's init fragment after that package's code.

No linker decision may depend on a virtual path prefix such as `<builtin>`.
Stdlib is a set of ordinary embedded package artifacts supplied by the CLI.
