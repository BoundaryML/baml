// Tests for `baml.crypto.HmacSha256` and `baml.crypto.constant_time_eq`.
//
// Expected tags are RFC 4231 vectors and the example from GitHub's webhook
// documentation, cross-checked against Python's `hmac` module.

// ─── Known answers ──────────────────────────────────────────────────────────

test "hmac_sha256_rfc4231_case_1" {
    let mac = baml.crypto.HmacSha256.new(uint8array.from_hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"));
    mac.update("Hi There".to_utf8());
    assert.equal(
        mac.finish().to_hex(),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
    )
}

test "hmac_sha256_rfc4231_case_2" {
    let mac = baml.crypto.HmacSha256.new("Jefe".to_utf8());
    mac.update("what do ya want for nothing?".to_utf8());
    assert.equal(
        mac.finish().to_hex(),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
    )
}

test "hmac_sha256_github_webhook_example" {
    // GitHub's documented example: secret "It's a Secret to Everybody",
    // payload "Hello, World!", header `X-Hub-Signature-256: sha256=<tag>`.
    let mac = baml.crypto.HmacSha256.new("It's a Secret to Everybody".to_utf8());
    mac.update("Hello, World!".to_utf8());
    let header = "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";
    let expected = uint8array.from_hex(header.slice(7, header.length()));
    assert.equal(baml.crypto.constant_time_eq(mac.finish(), expected), true)
}

test "hmac_sha256_empty_key_and_message" {
    let mac = baml.crypto.HmacSha256.new(b"");
    assert.equal(
        mac.finish().to_hex(),
        "b613679a0814d9ec772f95d778c35fc5ff1697c493715653c6c712144292c5ad",
    )
}

test "hmac_sha256_key_longer_than_a_block" {
    // 100 bytes is past SHA-256's 64-byte block, so the key is hashed first.
    let mac = baml.crypto.HmacSha256.new("k".repeat(100).to_utf8());
    mac.update("long key".to_utf8());
    assert.equal(
        mac.finish().to_hex(),
        "5903eedefe0bba3f15b658715e41d71f1848739bddf19e497e968df5723dcd56",
    )
}

// ─── Chunking and reset ─────────────────────────────────────────────────────

test "hmac_sha256_split_updates_match_one_update" {
    let split = baml.crypto.HmacSha256.new("Jefe".to_utf8());
    split.update("what do ya ".to_utf8());
    split.update("".to_utf8());
    split.update("want for nothing?".to_utf8());
    let whole = baml.crypto.HmacSha256.new("Jefe".to_utf8());
    whole.update("what do ya want for nothing?".to_utf8());
    assert.equal(split.finish(), whole.finish())
}

test "hmac_sha256_finish_resets_under_the_same_key" {
    let mac = baml.crypto.HmacSha256.new("Jefe".to_utf8());
    mac.update("what do ya want for nothing?".to_utf8());
    let first = mac.finish().to_hex();
    assert.equal(first, "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    // Nothing fed since the reset: the tag of the empty message under "Jefe".
    let empty = baml.crypto.HmacSha256.new("Jefe".to_utf8());
    assert.equal(mac.finish(), empty.finish());
    mac.update("what do ya want for nothing?".to_utf8());
    assert.equal(mac.finish().to_hex(), first)
}

test "hmac_sha256_different_keys_differ" {
    let a = baml.crypto.HmacSha256.new("key-a".to_utf8());
    let b = baml.crypto.HmacSha256.new("key-b".to_utf8());
    a.update("same message".to_utf8());
    b.update("same message".to_utf8());
    let tag_a = a.finish();
    assert.equal(tag_a.length(), 32);
    assert.is_true(tag_a.to_hex() != b.finish().to_hex())
}

// ─── Handle semantics ───────────────────────────────────────────────────────

function feed_mac(m: baml.crypto.HmacSha256, data: uint8array) -> void {
    m.update(data)
}

test "hmac_sha256_bindings_share_one_mac" {
    let m = baml.crypto.HmacSha256.new("Jefe".to_utf8());
    let alias = m;
    m.update("what do ya ".to_utf8());
    alias.update("want for ".to_utf8());
    feed_mac(m, "nothing?".to_utf8());
    assert.equal(
        m.finish().to_hex(),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
    )
}

// ─── constant_time_eq ───────────────────────────────────────────────────────

test "constant_time_eq_semantics" {
    assert.equal(baml.crypto.constant_time_eq(b"", b""), true);
    assert.equal(baml.crypto.constant_time_eq(b"abc", b"abc"), true);
    assert.equal(baml.crypto.constant_time_eq(b"abc", b"abd"), false);
    assert.equal(baml.crypto.constant_time_eq(b"abc", b"ab"), false);
    assert.equal(baml.crypto.constant_time_eq(b"", b"a"), false)
}
