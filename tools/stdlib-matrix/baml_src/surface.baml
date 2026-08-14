// Naming helpers shared by the two symbol layers.
//
// What survives of the old container-centric pooling: how an export id is
// split, and how a companion class maps to the type it backs. The pooling is
// gone, and so is the name normalization it existed for — nothing matches on
// spelling any more.

/// `"<K>:<dotted.path>"` → `"<dotted.path>"`.
function strip_kind_prefix(id: string) -> string {
    let out = id;
    if let idx: int = id.index_of(":") {
        out = id.slice(idx + 1, id.length());
    }
    out
}

function last_segment(path: string) -> string {
    let cut = -1;
    let i = 0;
    while (i < path.length()) {
        if (path.at(i) == ".") {
            cut = i;
        }
        i += 1;
    }
    path.slice(cut + 1, path.length())
}

/// Rewrites a companion class's name to the type it backs, leaving every other
/// type alone.
///
/// The stdlib backs each primitive with a companion class (`class baml.String`
/// carries `string`'s methods, `class Array<T>` carries `T[]`'s) — an
/// implementation detail of where methods live, not a type users write. The
/// matrix compares languages as written, so a display of `baml.Uint8Array`
/// becomes `uint8array`, including a `self` parameter's type.
///
/// A whole display, never a name inside a composite: `baml.String[]` would pass
/// through untouched. That costs nothing today because the export's canonical
/// renderer already normalizes companions before they reach here — no composite
/// companion spelling appears anywhere in it — so this only has to handle the
/// bare case the export does produce. The table mirrors
/// `PrimitiveType::builtin_class_path` and `head.rs`'s container normalization.
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
