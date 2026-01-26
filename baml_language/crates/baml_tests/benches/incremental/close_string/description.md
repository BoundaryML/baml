# Close String

Tests incremental parsing when fixing an unclosed string literal.

Expected behavior:
- Parser should recover from the error state
- Class structure should be preserved
- Field should be properly parsed after fix
