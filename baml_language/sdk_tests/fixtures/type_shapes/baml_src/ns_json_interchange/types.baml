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

// `@json` attributes shape a class's JSON the same in `baml.json` and serde.
// The union is open: an object no member's tag matches is `WireOther`, and a
// member whose tag matches but whose fields do not is an error.
class WireDefault {
    type: "use-default",
}

class WireVpc {
    type: "byo-vpc",
    vpc_id_field: string @json.rename("vpc-id"),
    subnets: string[]? @json.rename("subnet_ids"),

    @@json.omit_nulls
}

class WireOther {
    type: "UNKNOWN",
    raw: map<string, json> @json.fallback,
}

// A tag whose kind differs between a member and the fallback.
class WireStringCode {
    code: "two",
}

class WireNumberOther {
    code: 0,
    raw: map<string, json> @json.fallback,
}

class WireCoded {
    value: WireStringCode | WireNumberOther,
}

enum WireMode {
    Fast @json.rename("fast-mode")
    Slow
}

class WireSettings {
    network: (WireDefault | WireVpc | WireOther)?,
    mode: WireMode,
    name: string?,

    @@json.omit_nulls
}

// A union member reached through an alias keeps its tag.
type WireDefaultAlias = WireDefault

class WireAliased {
    network: WireDefaultAlias | WireOther,
}

// An optional literal under `@@json.omit_nulls`.
class WireOptionalTag {
    mode: "forward"?,
    name: string,

    @@json.omit_nulls
}

class WireConfig {
    id: string,
    kind: string?,
    extra: map<string, json> @json.flatten,

    @@json.omit_nulls
}
