# Failure and cancellation probe

This repository-only .NET 10 executable freezes the question-16 mechanics
before the product exception layer is implemented. It covers the complete
public inheritance/sealing contract, immutable ordered rendered trace lines,
decoded thrown/panic value identity, the exact panic exit metadata carried by
the outbound envelope, default-format redaction, custom canceled-task
behavior, all operation cancellation origins, direct synchronous rethrow,
exact callback exception identity and stack restoration, unrelated-token
fault classification, atomic terminal races, and `Environment.Exit` only in a
child process. It deliberately does not expose structured trace frames,
type-mismatch expected/actual/path fields, or panic category/reason/location
fields because the current outbound protocol does not carry them.
