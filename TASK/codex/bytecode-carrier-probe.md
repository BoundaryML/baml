# C# bytecode-carrier probe

Date: 2026-07-16

## Decision

The source-in-project generator emits exactly one
`BamlGeneratedProgram.g.cs`. It carries standard base64 in 12,000-character
constants and calls the hidden generated-code contract
`BamlBridge.RegisterEncodedProgram`. Raw bytecode is limited to 8 MiB at
generation and checked again by the runtime.

This is the v1 carrier. An embedded manifest resource or generated binary asset
would require the generator to edit or own MSBuild project integration. That is
incompatible with the selected zero-configuration source-in-project artifact.
The bounded source carrier participates in the existing generated-file manifest
and needs no resource name, copy target, or package-specific loader.

## Representative consumer

The `primitive_calls` fixture measured:

| Item | Bytes |
| --- | ---: |
| Raw bytecode | 633,774 |
| Base64 text | 845,032 |
| `BamlGeneratedProgram.g.cs` | 849,548 |
| Release consumer assembly | 3,433,472 |

The 8 MiB boundary was also compiled rather than inferred. Its 8,388,608 raw
bytes produced 11,184,812 base64 characters, 933 constants, and an 11,237,286
byte source file. A warning-free Release build completed in 3.50 seconds with
216,676 KiB peak RSS. Generation rejects 8 MiB plus one byte before emitting
files.

## Loading and diagnostics

The runtime validates the segment set and decoded length, allocates the raw
byte array once, and decodes each segment directly into its destination span.
It does not concatenate the encoded strings. `RegisterProgram` then verifies
the generated SHA-256 fingerprint before native initialization.

Managed tests cover missing, empty, malformed, internally padded, oversized,
and fingerprint-mismatched carriers. They receive stable
`BamlBridgeException` diagnostics before native initialization. A missing
central program source is a compile-time failure because every generated leaf
references it. The atomic generated-file manifest owns the one carrier file,
so stale cleanup and edited-file refusal use the same transaction as all other
generated C# source.

## Consumer probes

The regenerated project-reference fixture compiled with nullable warnings as
errors and both focused native consumers passed under nextest. A clean consumer
then restored only `baml-bridge 0.15.0` and `Google.Protobuf 3.35.1` from an
isolated local feed, compiled the same generated source against the package,
and executed sync and async calls through the packaged Linux asset. Its
application assembly was 3,412,480 bytes.

A framework-dependent, non-trimmed single-file publish for
`ubuntu.26.04-x64` succeeded and ran against the native bridge. Two publishes
produced byte-identical 4,848,261-byte executables with SHA-256
`352a2098ded6915a8a2b597e1d9cabfaba6bb5682100d63141d4e4bb9eda826f`.
The first publish took 3.41 seconds and 179,820 KiB peak RSS; the warm repeat
took 0.82 seconds and 152,896 KiB.

Trimming and NativeAOT are explicit v1 non-goals because the bridge's nominal,
generic, union, callback, and dynamic codecs use reflection. The carrier adds
no separate trimming promise. Non-trimmed ordinary, project-reference,
package-reference, and single-file paths are the supported evidence here.
