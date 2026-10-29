// End-to-end fixture for instances the runtime keeps to itself, here a
// `baml.hash.DefaultHasher`: an instance crosses to the host as a reference
// and comes back as the same instance, wherever a class can sit. Its state
// shows which instance a call reached: one hasher fed "a" and then "b" hashes
// as a fresh one fed "ab".

// A fresh hasher, with nothing written to it.
function make_hasher() -> baml.hash.DefaultHasher {
    baml.hash.DefaultHasher.new()
}

// Writes `text` to `hasher` and returns the hash of all it has been fed.
function hash_after(hasher: baml.hash.DefaultHasher, text: string) -> int {
    hasher.write(text.to_utf8());
    hasher.finish()
}

// The hasher it was given.
function same_hasher(hasher: baml.hash.DefaultHasher) -> baml.hash.DefaultHasher {
    hasher
}

function maybe_hasher(present: bool) -> baml.hash.DefaultHasher? {
    if (present) { make_hasher() } else { null }
}

// The hasher it was given, twice.
function hasher_twice(hasher: baml.hash.DefaultHasher) -> baml.hash.DefaultHasher[] {
    [hasher, hasher]
}

// The hasher it was given, under `key`.
function hasher_under(key: string, hasher: baml.hash.DefaultHasher) -> map<string, baml.hash.DefaultHasher> {
    let hashers: map<string, baml.hash.DefaultHasher> = {};
    hashers[key] = hasher;
    hashers
}

class HasherHolder {
    label string
    hasher baml.hash.DefaultHasher
}

function hold_hasher(label: string, hasher: baml.hash.DefaultHasher) -> HasherHolder {
    HasherHolder { label: label, hasher: hasher }
}

class HasherMarker {
    tag string
}

function hasher_or_marker(want_hasher: bool) -> baml.hash.DefaultHasher | HasherMarker {
    if (want_hasher) { make_hasher() } else { HasherMarker { tag: "marker" } }
}

function describe_hasher_or_marker(value: baml.hash.DefaultHasher | HasherMarker) -> string {
    match (value) {
        let hasher: baml.hash.DefaultHasher => "hasher",
        let marker: HasherMarker => marker.tag,
    }
}

// Hands `hasher` to the host's `callback` and returns what it gave back.
function hasher_through_callback(
    hasher: baml.hash.DefaultHasher,
    callback: (baml.hash.DefaultHasher) -> baml.hash.DefaultHasher,
) -> baml.hash.DefaultHasher {
    callback(hasher)
}
