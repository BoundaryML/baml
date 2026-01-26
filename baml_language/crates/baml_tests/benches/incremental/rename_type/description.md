# Rename Type Benchmark

Measures incremental compilation when renaming a type that's used across multiple files.

## Change
- Renamed `User` class to `Person`
- Updated all references in types and functions

## Expected Impact
- Should trigger recompilation of all affected files
- Type resolution needs to update references
- Cross-file dependency tracking test