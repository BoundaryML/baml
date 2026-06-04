// Minimal fixture for the size-gate `packed-program` artifact.
//
// `baml pack` produces a standalone executable: the `baml-pack-host`
// binary with a borsh-serialized bytecode envelope appended. Packing a
// trivial, pure function isolates the *host binary + envelope overhead*
// — the floor of what a user ships with `baml pack`. Keep this as small
// and dependency-free as possible so the measured size tracks the host
// binary, not the program. Editing it shifts the baseline.
function Identity(value: string) -> string {
    return value;
}
