# BAML C# RID-diagnostic probe

This repository-only fixture compiles the bounded v1 runtime-platform policy:
all eight supported OS/architecture/libc combinations map exactly, unsupported
combinations throw `PlatformNotSupportedException` with detected facts and the
complete supported list, and no architecture or libc family is substituted.
The exact-package build target separately validates explicit
`RuntimeIdentifier` inputs with `BAML0010`.
