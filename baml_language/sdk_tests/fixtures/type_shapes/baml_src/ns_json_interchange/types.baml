// JSON interchange: a host SDK's own JSON for these types must match BAML's
// (`baml.json.stringify`) and read back what `baml.json.decode` reads.
// The Rust SDK checks its optional `serde` impls against `to_json` and
// `from_json`.

enum Shade {
    Light,
    Dark,
}

class Label {
    text: string,
    note: string?,
}

class JsonDoc {
    title: string,
    count: int,
    ratio: float,
    done: bool,
    shade: Shade,
    subtitle: string?,
    tags: string[],
    scores: map<string, int>,
    label: Label,
    labels: Label[],
    amount: int | float,
    pick: int | Label,
    speed: "fast" | "slow",
    max_speed: ("fast" | "slow")?,
    // Rust keywords: `type` becomes `r#type`, `crate` becomes `crate_`.
    type: string,
    crate: int,
}

// `json` fields hold untyped JSON (`serde_json::Value` in Rust).
class JsonFields {
    payload: json,
    extra: json?,
    items: json[],
}

function to_json<T>(value: T) -> string throws baml.json.SerializationError {
    baml.json.stringify(value)
}

function from_json<T>(source: string) -> T throws baml.json.ParseError | baml.json.DecodeError {
    baml.json.decode<T>(source)
}
