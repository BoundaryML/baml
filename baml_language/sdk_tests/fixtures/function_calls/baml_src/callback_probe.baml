// Regression for async host callbacks whose BAML contract cannot throw.
// `throws never` constrains errors, not whether the host result is awaited.
function callback_probe(callback: (value: string) -> string throws never) -> string {
    callback("native callback")
}
