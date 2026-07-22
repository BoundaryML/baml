# BAML C# reflection-root probe

This repository-only fixture demonstrates the application-owned trimming
boundary for reflection over generated public types. A
`DynamicallyAccessedMembers` root retains the public constructor and
properties; an otherwise unreachable reflection-only type is deliberately
removed. The bridge itself does not discover or construct generated members
through arbitrary reflection.
