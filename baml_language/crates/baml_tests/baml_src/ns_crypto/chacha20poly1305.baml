// Tests for `baml.crypto.ChaCha20Poly1305` and `baml.crypto.XChaCha20Poly1305`.
//
// Both known-answer vectors share the key, aad, and plaintext that RFC 8439 and
// draft-irtf-cfrg-xchacha use, and differ only in nonce length and expected
// output. The ChaCha20-Poly1305 vector was cross-checked against OpenSSL, so it
// does not just restate what this implementation produces; OpenSSL has no
// XChaCha20-Poly1305, so that one is the published draft's vector.
//
// A round trip alone would not catch `plaintext` and `aad` being wired to the
// wrong fields of the underlying payload, since both directions would agree with
// each other and with no other implementation. The vectors pin the wiring.

// ─── Known answers ──────────────────────────────────────────────────────────

test "chacha20poly1305_matches_rfc8439_vector" {
    let cipher = baml.crypto.ChaCha20Poly1305.new(
        baml.Uint8Array.from_hex(
            "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f",
        ),
    );
    let sealed = cipher.encrypt(
        baml.Uint8Array.from_hex("070000004041424344454647"),
        "Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.".to_utf8(),
        baml.Uint8Array.from_hex("50515253c0c1c2c3c4c5c6c7"),
    );
    assert.equal(
        sealed.to_hex(),
        "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b61161ae10b594f09e26a7e902ecbd0600691",
    )
}

test "chacha20poly1305_decrypts_rfc8439_vector" {
    // The reverse direction: a ciphertext this implementation never produced
    // still opens to the standard's plaintext.
    let cipher = baml.crypto.ChaCha20Poly1305.new(
        baml.Uint8Array.from_hex(
            "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f",
        ),
    );
    let opened = cipher.decrypt(
        baml.Uint8Array.from_hex("070000004041424344454647"),
        baml.Uint8Array.from_hex(
            "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b61161ae10b594f09e26a7e902ecbd0600691",
        ),
        baml.Uint8Array.from_hex("50515253c0c1c2c3c4c5c6c7"),
    );
    assert.equal(
        string.from_utf8(opened),
        "Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.",
    )
}

test "xchacha20poly1305_matches_draft_vector" {
    let cipher = baml.crypto.XChaCha20Poly1305.new(
        baml.Uint8Array.from_hex(
            "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f",
        ),
    );
    let sealed = cipher.encrypt(
        baml.Uint8Array.from_hex("404142434445464748494a4b4c4d4e4f5051525354555657"),
        "Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.".to_utf8(),
        baml.Uint8Array.from_hex("50515253c0c1c2c3c4c5c6c7"),
    );
    assert.equal(
        sealed.to_hex(),
        "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b4522f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff921f9664c97637da9768812f615c68b13b52ec0875924c1c7987947deafd8780acf49",
    )
}

// ─── Nonce lengths ──────────────────────────────────────────────────────────

test "chacha20poly1305_takes_a_12_byte_nonce" {
    // The two variants differ only in nonce width, so each must reject the
    // other's. A cipher that quietly accepted both would produce ciphertexts no
    // peer could reproduce.
    let cipher = baml.crypto.ChaCha20Poly1305.new(baml.Uint8Array.zeroes(32));
    {
        cipher.encrypt(baml.Uint8Array.zeroes(24), b"msg", b"");
        baml.sys.panic("expected a 24-byte nonce to throw InvalidArgument")
    } catch (e) {
        baml.errors.InvalidArgument => assert.equal(
            e.message,
            "ChaCha20-Poly1305: nonce must be exactly 12 bytes, got 24",
        )
    }
}

test "xchacha20poly1305_takes_a_24_byte_nonce" {
    let cipher = baml.crypto.XChaCha20Poly1305.new(baml.Uint8Array.zeroes(32));
    {
        cipher.encrypt(baml.Uint8Array.zeroes(12), b"msg", b"");
        baml.sys.panic("expected a 12-byte nonce to throw InvalidArgument")
    } catch (e) {
        baml.errors.InvalidArgument => assert.equal(
            e.message,
            "XChaCha20-Poly1305: nonce must be exactly 24 bytes, got 12",
        )
    }
}

// ─── Round trips ────────────────────────────────────────────────────────────

test "chacha20poly1305_round_trips_through_the_aead_interface" {
    // Key and nonce from the real system CSPRNG, and both directions reached
    // through an `Aead`-typed parameter so the `VirtualCall` resolves the impl
    // on the runtime class. `seal_and_open` lives in `aes_gcm_siv.baml`; the
    // point here is that a second cipher family satisfies the same contract.
    let rng = baml.random.SystemRandom.get().as<baml.random.Rng>;
    let cipher = baml.crypto.ChaCha20Poly1305.new(baml.crypto.ChaCha20Poly1305.random_key(rng));
    assert.is_true(
        seal_and_open(
            cipher.as<baml.crypto.Aead>,
            rng.random(12),
            b"attack at dawn",
            b"envelope-v1",
        ),
    )
}

test "xchacha20poly1305_round_trips_through_the_aead_interface" {
    let rng = baml.random.SystemRandom.get().as<baml.random.Rng>;
    let cipher = baml.crypto.XChaCha20Poly1305.new(baml.crypto.XChaCha20Poly1305.random_key(rng));
    assert.is_true(
        seal_and_open(
            cipher.as<baml.crypto.Aead>,
            rng.random(24),
            b"attack at dawn",
            b"envelope-v1",
        ),
    )
}

test "chacha20poly1305_ciphertext_is_plaintext_plus_a_tag" {
    // The tag is a fixed 16-byte overhead, and `aad` is authenticated but not
    // encrypted, so it must not change the ciphertext's length.
    let cipher = baml.crypto.ChaCha20Poly1305.new(baml.Uint8Array.zeroes(32));
    let nonce = baml.Uint8Array.zeroes(12);
    let message = b"sixteen bytes!!!";
    let no_aad = cipher.encrypt(nonce, message, baml.Uint8Array.zeroes(0));
    let long_aad = cipher.encrypt(nonce, message, baml.Uint8Array.zeroes(4096));
    assert.equal(no_aad.length(), message.length() + 16);
    assert.equal(long_aad.length(), no_aad.length())
}

// ─── Nonce misuse ───────────────────────────────────────────────────────────

test "chacha20poly1305_is_not_misuse_resistant" {
    // The contrast with `Aes256GcmSiv` that the class documentation warns about.
    // Reusing a `(key, nonce)` pair makes the keystream identical, so the XOR of
    // two ciphertexts is the XOR of their plaintexts. Here two messages that
    // differ in one byte produce ciphertexts that differ in exactly that byte,
    // which is the leak stated plainly.
    let cipher = baml.crypto.ChaCha20Poly1305.new(baml.Uint8Array.zeroes(32));
    let nonce = baml.Uint8Array.zeroes(12);
    let a = cipher.encrypt(nonce, b"AAAAAAAA", b"").to_array();
    let b = cipher.encrypt(nonce, b"AAAAAAAB", b"").to_array();
    // Ciphertext bytes 0..6 cover the identical plaintext prefix.
    assert.equal(a[0], b[0]);
    assert.equal(a[6], b[6]);
    // Byte 7 covers the one plaintext byte that differs, and differs by the same
    // XOR: 'A' ^ 'B' == 3.
    assert.equal(a[7] ^ b[7], 3)
}

// ─── Rejected ciphertexts ───────────────────────────────────────────────────

test "chacha20poly1305_wrong_aad_is_rejected" {
    let cipher = baml.crypto.ChaCha20Poly1305.new(baml.Uint8Array.zeroes(32));
    let nonce = baml.Uint8Array.zeroes(12);
    let sealed = cipher.encrypt(nonce, b"secret", b"envelope-v1");
    {
        cipher.decrypt(nonce, sealed, b"envelope-v2");
        baml.sys.panic("expected decrypt under the wrong aad to throw DecryptionFailure")
    } catch (e) {
        baml.crypto.DecryptionFailure => {
            assert.equal(e.algorithm, "ChaCha20-Poly1305");
            assert.equal(e.reason, "authentication failed")
        }
    }
}

test "xchacha20poly1305_tampered_ciphertext_is_rejected" {
    // Flipping one bit of the ciphertext body must fail authentication rather
    // than yield a corrupted plaintext.
    let cipher = baml.crypto.XChaCha20Poly1305.new(baml.Uint8Array.zeroes(32));
    let nonce = baml.Uint8Array.zeroes(24);
    let empty = baml.Uint8Array.zeroes(0);
    let bytes = cipher.encrypt(nonce, b"secret", empty).to_array();
    bytes[0] = bytes[0] ^ 1;
    let tampered = baml.Uint8Array.from_array(bytes);
    {
        cipher.decrypt(nonce, tampered, empty);
        baml.sys.panic("expected a tampered ciphertext to throw DecryptionFailure")
    } catch (e) {
        baml.crypto.DecryptionFailure => {
            assert.equal(e.algorithm, "XChaCha20-Poly1305");
            assert.equal(e.reason, "authentication failed")
        }
    }
}

test "chacha20poly1305_key_from_another_cipher_does_not_open_it" {
    // Both ChaCha variants take a 32-byte key, so nothing about the key length
    // stops a caller from crossing them. Authentication has to be what refuses.
    let key = baml.Uint8Array.zeroes(32);
    let sealed = baml.crypto.ChaCha20Poly1305.new(key).encrypt(
        baml.Uint8Array.zeroes(12),
        b"secret",
        b"",
    );
    {
        baml.crypto.XChaCha20Poly1305.new(key).decrypt(
            baml.Uint8Array.zeroes(24),
            sealed,
            b"",
        );
        baml.sys.panic("expected XChaCha20 to reject a ChaCha20 ciphertext")
    } catch (e) {
        baml.crypto.DecryptionFailure => assert.equal(e.reason, "authentication failed")
    }
}

// ─── Malformed calls ────────────────────────────────────────────────────────

test "chacha20poly1305_wrong_key_length_is_invalid_argument" {
    {
        baml.crypto.ChaCha20Poly1305.new(baml.Uint8Array.zeroes(16));
        baml.sys.panic("expected a 16-byte key to throw InvalidArgument")
    } catch (e) {
        baml.errors.InvalidArgument => assert.equal(
            e.message,
            "ChaCha20-Poly1305: key must be exactly 32 bytes, got 16",
        )
    }
}

test "chacha20poly1305_ciphertext_too_short_to_hold_a_tag_is_named" {
    let cipher = baml.crypto.ChaCha20Poly1305.new(baml.Uint8Array.zeroes(32));
    {
        cipher.decrypt(
            baml.Uint8Array.zeroes(12),
            baml.Uint8Array.zeroes(15),
            baml.Uint8Array.zeroes(0),
        );
        baml.sys.panic("expected a 15-byte ciphertext to throw DecryptionFailure")
    } catch (e) {
        baml.crypto.DecryptionFailure => assert.equal(
            e.reason,
            "ciphertext is shorter than the 16-byte authentication tag",
        )
    }
}

// ─── Key generation ─────────────────────────────────────────────────────────

test "chacha_random_key_draws_32_bytes" {
    let rng = baml.random.SystemRandom.get().as<baml.random.Rng>;
    assert.equal(baml.crypto.ChaCha20Poly1305.random_key(rng).length(), 32);
    assert.equal(baml.crypto.XChaCha20Poly1305.random_key(rng).length(), 32)
}
