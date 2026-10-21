// Map literals adopt the expected key/value types
//
// With an expected map type, entries CHECK against the expected key
// and value (the Array arm's rule; r-a's expectation-driven literal
// typing), so `{"input": input}` in a `map<string, unknown>` position
// IS `map<string, unknown>` - not a synthesized `map<string, string>`
// tripping the invariance rule.
function ml_args(input: string) -> map<string, unknown> throws never {
    let m: map<string, unknown> = {"input": input};
     // ^ map<string, unknown>
    m
}
