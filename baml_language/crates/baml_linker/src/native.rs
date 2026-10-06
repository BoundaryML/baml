//! Native code has edges the bytecode walker cannot see. The audited key list
//! is closed: an unknown native conservatively requires the complete image.
//! Generated glue dependencies come from the same declarations that generate
//! Rust class marshalling. The cases below cover name lookup and interface
//! resolution inside handwritten Rust implementations.
include!(concat!(env!("OUT_DIR"), "/native_dependencies.rs"));

pub(super) fn dependencies(key: &str) -> Option<Vec<&'static str>> {
    let mut deps = generated_dependencies(key)?.to_vec();
    let extra: &[&str] = match key {
        "ai.Prompt.messages" => &[
            "ai.Prompt",
            "ai.PromptMessage",
            "ai.CacheDelimiter",
            "baml.media.Image",
            "baml.media.Audio",
            "baml.media.Video",
            "baml.media.Pdf",
        ],
        "baml._to_string_default" => &["baml.ToString"],
        "baml._to_json_default" | "baml.json.to_json" => &["baml.ToJson"],
        "baml._from_json_structural_default" | "baml.json.from_json" => &[
            "baml.FromJson",
            "baml.json.to",
            "baml.media.Image",
            "baml.media.Audio",
            "baml.media.Video",
            "baml.media.Pdf",
        ],
        "baml.ops.equals_equals" | "baml._equals_structural_default" => &["baml.ops.Equals"],
        "baml._hash_structural_default" => {
            &["baml.Hash", "baml.hash.Hasher", "baml.hash.DefaultHasher"]
        }
        "baml.ops.__union_add" => &["baml.ops.Add"],
        "baml.ops.__union_sub" => &["baml.ops.Subtract"],
        "baml.ops.__union_mul" => &["baml.ops.Multiply"],
        "baml.ops.__union_div" => &["baml.ops.Divide"],
        "baml.ops.__union_rem" => &["baml.ops.Remainder"],
        "baml.ops.__union_neg" => &["baml.ops.Negate"],
        "baml.toml.Table.parse" | "baml.toml.Table._to_string_impl" => &[
            "baml.toml.Table",
            "baml.toml.Item",
            "baml.toml.ParseError",
            "baml.time.PlainDate",
            "baml.time.PlainDateTime",
            "baml.time.PlainTime",
            "baml.time.ZonedDateTime",
        ],
        "baml.csv.Reader._poll" | "baml.csv.Reader._poll_headers" => &["baml.csv._NeedData"],
        "baml.yaml.parse" => &["baml.yaml.ParseError"],
        _ => &[],
    };
    deps.extend_from_slice(extra);
    // Native construction/error paths may erase their result to a Value or
    // json. Keep these audited groups even when no signature names them.
    if key.starts_with("baml.json.")
        || key.starts_with("baml._to_json")
        || key == "baml._from_json_structural_default"
    {
        deps.extend_from_slice(&[
            "baml.json.ParseError",
            "baml.json.DecodeError",
            "baml.json.SerializationError",
        ]);
    }
    if key.starts_with("baml.csv.") {
        deps.extend_from_slice(&[
            "baml.csv.Error",
            "baml.csv.ErrorKind",
            "baml.csv.Record",
            "baml.csv.Position",
            "baml.csv._Headers",
            "baml.csv._Skip",
            "baml.csv._NeedData",
            "baml.iter.Done",
            "baml.time.Instant",
            "baml.time.PlainDate",
            "baml.time.PlainDateTime",
        ]);
    }
    if key.starts_with("baml.regex.") {
        deps.extend_from_slice(&[
            "baml.regex.Regex",
            "baml.regex.Match",
            "baml.regex.Group",
            "baml.regex.Error",
            "baml.regex.ErrorKind",
        ]);
    }
    if key.starts_with("baml.crypto.") {
        deps.push("baml.crypto.DecryptionFailure");
    }
    if key == "baml.future.Future.state" {
        deps.push("baml.future.State");
    }
    // Native map dispatch and structural hashing can reach arbitrary user Hash
    // and Equals overrides, including nested keys, through Rust continuations.
    if key.starts_with("baml.Map.") {
        deps.extend_from_slice(&[
            "baml.Hash",
            "baml.hash.Hasher",
            "baml.hash.DefaultHasher",
            "baml.ops.Equals",
        ]);
    }
    Some(deps)
}
