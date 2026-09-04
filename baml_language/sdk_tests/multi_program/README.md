# Multiple generated SDKs in one process

Build the local CLI and Python/Node bridges, then run `python3 baml_language/sdk_tests/multi_program/run.py` from the repository root. See [requirements and design](../../architecture/multiple-runtime-registrations.md) for registration, identity, and lifetime semantics.

The runner generates A, B, and a relocated copy of A. It asserts stable identity across relocation, executes the Python regressions in one process, compiles the TypeScript fixtures, and executes them in one Node process. It uses the local editable Python package and local Node package, so published bridge binaries cannot hide an implementation regression.

CI runs the language-specific variants (`--language python` and `--language typescript`) in the Linux SDK jobs after building the local bridges. The Node runner requires both a successful exit status and the final completion marker, so early process termination cannot silently pass.
