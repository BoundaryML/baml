// The namespaced enum of `enum_variant_method_paths`, and a function that sits
// next to it so a path into this namespace can name either.

enum Tone {
    Warm
    Cool
}

function label(tone: Tone) -> string {
    "tone " + tone.to_string()
}
