# Add String Character

Tests incremental parsing when adding a single character to a string literal.

Expected behavior:
- Parser should reuse the class and field nodes
- Only the string literal node should be reparsed
- Node reuse should be >95%
