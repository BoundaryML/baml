// Naming helpers shared by the two symbol layers.
//
// These are the parts of the old container-centric pooling worth keeping: how
// a name is normalized for matching, how an export id is split, and how a
// companion class maps to the type it backs. The pooling itself is gone —
// symbols.baml and ts_symbols.baml flatten each side directly.

/// The deterministic match key: lowercase with underscores stripped, so BAML
/// snake_case meets TS camelCase (`to_upper_case` ≡ `toUpperCase`).
function match_key(name: string) -> string {
    name.to_lower_case().replace_all("_", "")
}

/// `"<K>:<dotted.path>"` → `"<dotted.path>"`.
function strip_kind_prefix(id: string) -> string {
    let out = id;
    if let idx: int = id.index_of(":") {
        out = id.substring(idx + 1, id.length());
    }
    out
}

function last_segment(path: string) -> string {
    let cut = -1;
    let i = 0;
    while (i < path.length()) {
        if (path.char_at(i) == ".") {
            cut = i;
        }
        i += 1;
    }
    path.substring(cut + 1, path.length())
}

/// Rewrites a companion class's name to the type it backs, leaving every other
/// type alone.
///
/// The stdlib backs each primitive with a companion class (`class baml.String`
/// carries `string`'s methods, `class Array<T>` carries `T[]`'s) — an
/// implementation detail of where methods live, not a type users write. The
/// matrix compares languages as written, so `baml.Uint8Array` normalizes to
/// `uint8array` wherever it appears, including a `self` parameter's type. The
/// table mirrors `PrimitiveType::builtin_class_path` and `head.rs`'s container
/// normalization.
function normalize_ty_display(display: string) -> string {
    match (display) {
        "baml.Int" => "int",
        "baml.Bigint" => "bigint",
        "baml.Float" => "float",
        "baml.Bool" => "bool",
        "baml.Null" => "null",
        "baml.String" => "string",
        "baml.Uint8Array" => "uint8array",
        "baml.media.Image" => "image",
        "baml.media.Audio" => "audio",
        "baml.media.Video" => "video",
        "baml.media.Pdf" => "pdf",
        "baml.Array" => "T[]",
        "baml.Map" => "map<K, V>",
        _ => display,
    }
}

/// How a type is written in source, given its declaring name. Companion
/// classes normalize to the type they back; everything else keeps its own
/// short name.
function type_display_name(qualified: string) -> string {
    let normalized = normalize_ty_display(qualified);
    if (normalized != qualified) {
        normalized
    } else {
        last_segment(qualified)
    }
}

/// A BAML member is "static" when it takes no `self` receiver. Members whose
/// signature failed to resolve are treated as instance members.
function is_static_member(sig: ExportSignature?) -> bool {
    let out = false;
    if let s: ExportSignature = sig {
        if (s.params.length() == 0) {
            out = true;
        } else {
            if let first: ExportParam = s.params.at(0) {
                out = first.name != "self";
            }
        }
    }
    out
}
