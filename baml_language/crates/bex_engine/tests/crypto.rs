//! End-to-end tests for the `baml.crypto` AEAD ciphers, run through the engine
//! with the real native `SysOps` (so `SystemRandom`-drawn keys and nonces
//! exercise the `$rust_io_function` path alongside the inline `$rust_function`
//! seal/open).

mod common;

use bex_engine::BexExternalValue;
use common::{EngineProgram, assert_engine_executes};

/// Run `source`'s `main` and assert it returns `expected`.
async fn assert_main(source: &'static str, expected: BexExternalValue) {
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        expected: Ok(expected),
        ..Default::default()
    })
    .await
    .unwrap();
}

/// Run `source`'s `main` and assert it returns the string `expected`.
async fn assert_main_str(source: &'static str, expected: &str) {
    assert_main(source, BexExternalValue::String(expected.into())).await;
}

/// Run `source`'s `main` and assert it returns `true`.
async fn assert_main_true(source: &'static str) {
    assert_main(source, BexExternalValue::Bool(true)).await;
}

// ─── Known-answer tests (RFC 8452 Appendix C) ────────────────────────────────
//
// A round trip alone would still pass if `plaintext` and `aad` were wired to
// the wrong `Payload` fields, since both directions would agree with each other
// and with nothing else. These vectors pin the wiring to the standard.

#[tokio::test]
async fn aes256_matches_rfc8452_vector() {
    let source = r#"
function main() -> string {
    let key = baml.Uint8Array.from_hex("0100000000000000000000000000000000000000000000000000000000000000");
    let nonce = baml.Uint8Array.from_hex("030000000000000000000000");
    let aad = baml.Uint8Array.from_hex("01");
    let plaintext = baml.Uint8Array.from_hex("0200000000000000");
    baml.crypto.Aes256GcmSiv.new(key).encrypt(nonce, plaintext, aad).to_hex()
}
"#;
    assert_main_str(source, "1de22967237a813291213f267e3b452f02d01ae33e4ec854").await;
}

#[tokio::test]
async fn aes128_matches_rfc8452_vector() {
    let source = r#"
function main() -> string {
    let key = baml.Uint8Array.from_hex("01000000000000000000000000000000");
    let nonce = baml.Uint8Array.from_hex("030000000000000000000000");
    let aad = baml.Uint8Array.from_hex("01");
    let plaintext = baml.Uint8Array.from_hex("0200000000000000");
    baml.crypto.Aes128GcmSiv.new(key).encrypt(nonce, plaintext, aad).to_hex()
}
"#;
    assert_main_str(source, "1e6daba35669f4273b0a1a2560969cdf790d99759abd1508").await;
}

#[tokio::test]
async fn aes256_decrypts_rfc8452_vector() {
    // The reverse direction of `aes256_matches_rfc8452_vector`: a ciphertext
    // this implementation never produced still opens to the standard's
    // plaintext.
    let source = r#"
function main() -> string {
    let key = baml.Uint8Array.from_hex("0100000000000000000000000000000000000000000000000000000000000000");
    let nonce = baml.Uint8Array.from_hex("030000000000000000000000");
    let aad = baml.Uint8Array.from_hex("01");
    let ciphertext = baml.Uint8Array.from_hex("1de22967237a813291213f267e3b452f02d01ae33e4ec854");
    baml.crypto.Aes256GcmSiv.new(key).decrypt(nonce, ciphertext, aad).to_hex()
}
"#;
    assert_main_str(source, "0200000000000000").await;
}

// ─── Round trips ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn round_trips_through_the_aead_interface() {
    // Keys and nonces from the real system CSPRNG, and both directions reached
    // through an `Aead`-typed parameter so the `VirtualCall` resolves the impl
    // on the runtime class.
    let source = r#"
function seal_and_open(cipher: baml.crypto.Aead, nonce: uint8array, message: uint8array, aad: uint8array) -> bool {
    cipher.decrypt(nonce, cipher.encrypt(nonce, message, aad), aad) == message
}
function main() -> bool {
    let rng = baml.random.SystemRandom.get().as<baml.random.Rng>;
    let key = baml.crypto.Aes256GcmSiv.random_key(rng);
    let cipher = baml.crypto.Aes256GcmSiv.new(key).as<baml.crypto.Aead>;
    seal_and_open(cipher, rng.random(12), b"attack at dawn", b"envelope-v1")
}
"#;
    assert_main_true(source).await;
}

#[tokio::test]
async fn round_trips_with_empty_plaintext_and_aad() {
    // An empty plaintext and an empty aad still produce a bare 16-byte tag,
    // which opens back to empty.
    let source = r#"
function main() -> bool {
    let empty = baml.Uint8Array.zeroes(0);
    let cipher = baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(32));
    let nonce = baml.Uint8Array.zeroes(12);
    let sealed = cipher.encrypt(nonce, empty, empty);
    sealed.length() == 16 && cipher.decrypt(nonce, sealed, empty).length() == 0
}
"#;
    assert_main_true(source).await;
}

#[tokio::test]
async fn ciphertext_is_plaintext_plus_a_tag() {
    // The tag is a fixed 16-byte overhead. `aad` is authenticated but not
    // encrypted, so it must not change the ciphertext's length.
    let source = r#"
function main() -> bool {
    let cipher = baml.crypto.Aes128GcmSiv.new(baml.Uint8Array.zeroes(16));
    let nonce = baml.Uint8Array.zeroes(12);
    let message = b"sixteen bytes!!!";
    let short = cipher.encrypt(nonce, message, baml.Uint8Array.zeroes(0));
    let long = cipher.encrypt(nonce, message, baml.Uint8Array.zeroes(4096));
    short.length() == message.length() + 16 && long.length() == short.length()
}
"#;
    assert_main_true(source).await;
}

#[tokio::test]
async fn oversized_key_is_rejected_not_truncated() {
    // A 32-byte key is an `Aes256GcmSiv` key and nothing else. Handing it to the
    // 128-bit cipher must fail rather than silently use its first 16 bytes,
    // which would produce ciphertexts no peer could reproduce.
    let source = r#"
function main() -> string {
    baml.crypto.Aes128GcmSiv.new(baml.Uint8Array.zeroes(32)) catch (e) {
        baml.errors.InvalidArgument { message } => {
            throw baml.errors.DevOther { message: message }
        },
    };
    "built a 128-bit cipher from a 32-byte key"
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        expected: Err("AES-128-GCM-SIV: key must be exactly 16 bytes, got 32"),
        ..Default::default()
    })
    .await
    .unwrap();
}

// ─── Nonce misuse ────────────────────────────────────────────────────────────

#[tokio::test]
async fn repeated_nonce_is_deterministic() {
    // The SIV property: a repeated `(key, nonce)` pair leaks only whether two
    // plaintexts were equal. Identical inputs give identical ciphertexts; a
    // changed plaintext or a changed aad does not.
    let source = r#"
function main() -> bool {
    let cipher = baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(32));
    let nonce = baml.Uint8Array.zeroes(12);
    let aad = b"ctx";
    let a = cipher.encrypt(nonce, b"same message", aad);
    let b = cipher.encrypt(nonce, b"same message", aad);
    let c = cipher.encrypt(nonce, b"different!!!", aad);
    let d = cipher.encrypt(nonce, b"same message", b"other-ctx");
    a == b && a != c && a != d
}
"#;
    assert_main_true(source).await;
}

// ─── Rejection ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn wrong_aad_is_rejected() {
    let source = r#"
function main() -> string {
    let cipher = baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(32));
    let nonce = baml.Uint8Array.zeroes(12);
    let sealed = cipher.encrypt(nonce, b"secret", b"envelope-v1");
    cipher.decrypt(nonce, sealed, b"envelope-v2") catch (e) {
        baml.crypto.DecryptionFailure { algorithm, reason } => {
            throw baml.errors.DevOther { message: algorithm + ": " + reason }
        },
    };
    "decrypted under the wrong aad"
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        expected: Err("AES-256-GCM-SIV: authentication failed"),
        ..Default::default()
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn tampered_ciphertext_is_rejected() {
    // Flipping one bit of the ciphertext body must fail authentication rather
    // than yield a corrupted plaintext.
    let source = r#"
function main() -> string {
    let cipher = baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(32));
    let nonce = baml.Uint8Array.zeroes(12);
    let sealed = cipher.encrypt(nonce, b"secret", baml.Uint8Array.zeroes(0));
    let bytes = sealed.to_array();
    bytes[0] = bytes[0] ^ 1;
    let tampered = baml.Uint8Array.from_array(bytes);
    cipher.decrypt(nonce, tampered, baml.Uint8Array.zeroes(0)) catch (e) {
        baml.crypto.DecryptionFailure { algorithm, reason } => {
            throw baml.errors.DevOther { message: reason }
        },
    };
    "decrypted a tampered ciphertext"
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        expected: Err("authentication failed"),
        ..Default::default()
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn ciphertext_too_short_to_hold_a_tag_is_named() {
    // A buffer too short to contain a tag is a `DecryptionFailure` (it is
    // ciphertext, not a malformed call), but its reason names the length, which
    // the caller already knows, instead of the opaque tag-mismatch text.
    let source = r#"
function main() -> string {
    let cipher = baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(32));
    cipher.decrypt(baml.Uint8Array.zeroes(12), baml.Uint8Array.zeroes(15), baml.Uint8Array.zeroes(0)) catch (e) {
        baml.crypto.DecryptionFailure { algorithm, reason } => {
            throw baml.errors.DevOther { message: reason }
        },
    };
    "decrypted a 15-byte ciphertext"
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        expected: Err("ciphertext is shorter than the 16-byte authentication tag"),
        ..Default::default()
    })
    .await
    .unwrap();
}

// ─── Malformed calls ─────────────────────────────────────────────────────────

#[tokio::test]
async fn wrong_key_length_is_invalid_argument() {
    let source = r#"
function main() -> string {
    baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(31)) catch (e) {
        baml.errors.InvalidArgument { message } => {
            throw baml.errors.DevOther { message: message }
        },
    };
    "built a cipher from a 31-byte key"
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        expected: Err("AES-256-GCM-SIV: key must be exactly 32 bytes, got 31"),
        ..Default::default()
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn wrong_nonce_length_is_invalid_argument() {
    // A malformed call, not a rejected ciphertext, so it is `InvalidArgument`
    // even on the decrypt side, where a `DecryptionFailure` would wrongly
    // suggest the data was at fault.
    let source = r#"
function main() -> string {
    let cipher = baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(32));
    cipher.decrypt(baml.Uint8Array.zeroes(16), baml.Uint8Array.zeroes(32), baml.Uint8Array.zeroes(0)) catch (e) {
        baml.errors.InvalidArgument { message } => {
            throw baml.errors.DevOther { message: message }
        },
    };
    "decrypted under a 16-byte nonce"
}
"#;
    assert_engine_executes(EngineProgram {
        source,
        entry: "main",
        expected: Err("AES-256-GCM-SIV: nonce must be exactly 12 bytes, got 16"),
        ..Default::default()
    })
    .await
    .unwrap();
}

// ─── Key generation ──────────────────────────────────────────────────────────

#[tokio::test]
async fn random_key_draws_the_algorithms_key_length() {
    let source = r#"
function main() -> bool {
    let rng = baml.random.SystemRandom.get().as<baml.random.Rng>;
    baml.crypto.Aes256GcmSiv.random_key(rng).length() == 32
        && baml.crypto.Aes128GcmSiv.random_key(rng).length() == 16
}
"#;
    assert_main_true(source).await;
}

#[tokio::test]
async fn random_key_feeds_new_directly() {
    // `GenerateKey` returns raw bytes so they can be stored or transported. The
    // contract is that the same algorithm's `new` accepts them as they are.
    let source = r#"
function main() -> bool {
    let rng = baml.random.ChaCha20.new().as<baml.random.Rng>;
    let key = baml.crypto.Aes128GcmSiv.random_key(rng);
    let cipher = baml.crypto.Aes128GcmSiv.new(key);
    let nonce = rng.random(12);
    cipher.decrypt(nonce, cipher.encrypt(nonce, b"round trip", b""), b"") == b"round trip"
}
"#;
    assert_main_true(source).await;
}
