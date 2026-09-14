// Tests for `baml.crypto.Sha256`.
//
// The expected digests are FIPS 180-4 vectors, cross-checked against Python's
// `hashlib.sha256` so they do not just restate what this implementation
// produces.
//
// A hasher is a handle onto mutable state, so every test builds its own with
// `Sha256.new()`.

// ─── Known answers ──────────────────────────────────────────────────────────

test "sha256_empty_message" {
    let h = baml.crypto.Sha256.new();
    assert.equal(
        h.finish().to_hex(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    )
}

test "sha256_abc" {
    let h = baml.crypto.Sha256.new();
    h.update("abc".to_utf8());
    assert.equal(
        h.finish().to_hex(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )
}

test "sha256_two_block_message" {
    // 56 bytes, which is long enough that the length padding spills past the
    // first 64-byte block and a second compression round runs.
    let h = baml.crypto.Sha256.new();
    h.update("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".to_utf8());
    assert.equal(
        h.finish().to_hex(),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    )
}

// ─── Chunking ───────────────────────────────────────────────────────────────

test "sha256_split_updates_match_one_update" {
    // How a caller chunks a message is not part of the message, empty chunks
    // included.
    let split = baml.crypto.Sha256.new();
    split.update("hel".to_utf8());
    split.update("".to_utf8());
    split.update("lo wor".to_utf8());
    split.update("ld".to_utf8());

    let whole = baml.crypto.Sha256.new();
    whole.update("hello world".to_utf8());

    // Read each digest once: `finish` resets, so a second call would return the
    // empty digest rather than repeat this one.
    let split_digest = split.finish().to_hex();
    assert.equal(
        split_digest,
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
    );
    assert.equal(whole.finish().to_hex(), split_digest)
}

// ─── finish resets ──────────────────────────────────────────────────────────

test "sha256_second_finish_is_the_empty_digest" {
    // `finish` resets, so a second call with nothing fed in between hashes the
    // empty message rather than repeating the first digest.
    let h = baml.crypto.Sha256.new();
    h.update("abc".to_utf8());
    assert.equal(
        h.finish().to_hex(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
    assert.equal(
        h.finish().to_hex(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    )
}

test "sha256_hasher_is_reusable_after_finish" {
    // The reset leaves the hasher equivalent to a fresh one, so feeding the same
    // message again repeats its digest.
    let h = baml.crypto.Sha256.new();
    h.update("abc".to_utf8());
    let first = h.finish().to_hex();
    h.update("abc".to_utf8());
    assert.equal(h.finish().to_hex(), first)
}

// ─── Digest width ───────────────────────────────────────────────────────────

test "sha256_digest_is_always_32_bytes" {
    let empty = baml.crypto.Sha256.new();
    assert.equal(empty.finish().length(), 32);

    let multi_block = baml.crypto.Sha256.new();
    multi_block.update("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".to_utf8());
    multi_block.update("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".to_utf8());
    assert.equal(multi_block.finish().length(), 32)
}

// ─── Interface dispatch ─────────────────────────────────────────────────────

// Takes `Hasher` rather than `Sha256` on purpose: reaching `update` through an
// interface-typed parameter is what these two tests are checking.
function feed_hasher(h: baml.crypto.Hasher, data: uint8array) -> void {
    h.update(data)
}

test "sha256_hashes_through_the_hasher_interface" {
    // Both methods reached through a `Hasher`-typed binding, so the
    // `VirtualCall` resolves the impl on the runtime class.
    let h = baml.crypto.Sha256.new().as<baml.crypto.Hasher>;
    h.update("abc".to_utf8());
    assert.equal(
        h.finish().to_hex(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )
}

test "sha256_bindings_share_one_hasher" {
    // A `Sha256` is a handle, not a value: binding it, and passing it into a
    // function, both feed the same state. Pinned because the other reading,
    // where a binding copies the hasher, would silently produce a different
    // digest.
    let h = baml.crypto.Sha256.new();
    let alias = h;
    h.update("hello".to_utf8());
    alias.update(" ".to_utf8());
    feed_hasher(h.as<baml.crypto.Hasher>, "world".to_utf8());
    assert.equal(
        h.finish().to_hex(),
        "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
    )
}
