# Add Attribute

Tests incremental parsing when adding an attribute to a field.

Expected behavior:
- Class node should be reused
- Field node might be reparsed but efficiently
- Overall node reuse should be >80%
