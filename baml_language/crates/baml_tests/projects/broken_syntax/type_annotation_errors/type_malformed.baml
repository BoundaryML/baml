// Test error recovery for malformed syntax involving `type`

// Incomplete type alias: missing everything after keyword
type

// Incomplete type alias: missing equals and value
type Foo

// Incomplete type alias: missing value after equals
type Bar =
