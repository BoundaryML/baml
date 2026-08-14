# Canonical bytecode carrier evidence emitter

This repository-only .NET 10 tool renders canonical compiler bytecode into the
question-20 carrier shape: one private hexadecimal `byte[]`, lowercase SHA-256
metadata, and one `Lazy` bootstrap. It exists to make verification gate B13
executable before the production C# generator is implemented.

For boundary evidence, `--synthesize <byte-count>` writes a deterministic
payload before emitting the same carrier. The payload is the little-endian
byte stream from xorshift64* with initial state `0x4d595df4d0f33173`,
the standard xorshift `(12, 25, 27)` transitions, and multiplier
`0x2545f4914f6cdd1d`. This mode is a reproducible compiler/toolchain probe,
not a product bytecode format or a product size limit.

It is not a runtime bytecode loader, a public bytecode format, or an automatic
MSBuild generation target.
