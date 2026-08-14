// Tests for `baml.crypto.Aes128GcmSiv` and `baml.crypto.Aes256GcmSiv`.
//
// The known-answer vectors are RFC 8452 Appendix C. They matter beyond a round
// trip: encrypting and decrypting would still agree with each other if
// `plaintext` and `aad` were wired to the wrong fields of the underlying
// payload, agreeing with each other and with no other implementation. The
// vectors pin the wiring to the standard.
//
// Throw tests use the `{ <body>; baml.sys.panic("…") } catch (e) { <Type> => … }`
// pattern.

// ─── Known answers (RFC 8452 Appendix C) ────────────────────────────────────

test "aes256_matches_rfc8452_vector" {
    let cipher = baml.crypto.Aes256GcmSiv.new(
        baml.Uint8Array.from_hex(
            "0100000000000000000000000000000000000000000000000000000000000000",
        ),
    );
    let sealed = cipher.encrypt(
        baml.Uint8Array.from_hex("030000000000000000000000"),
        baml.Uint8Array.from_hex("0200000000000000"),
        baml.Uint8Array.from_hex("01"),
    );
    assert.equal(sealed.to_hex(), "1de22967237a813291213f267e3b452f02d01ae33e4ec854")
}

test "aes128_matches_rfc8452_vector" {
    let cipher = baml.crypto.Aes128GcmSiv.new(
        baml.Uint8Array.from_hex("01000000000000000000000000000000"),
    );
    let sealed = cipher.encrypt(
        baml.Uint8Array.from_hex("030000000000000000000000"),
        baml.Uint8Array.from_hex("0200000000000000"),
        baml.Uint8Array.from_hex("01"),
    );
    assert.equal(sealed.to_hex(), "1e6daba35669f4273b0a1a2560969cdf790d99759abd1508")
}

test "aes256_decrypts_rfc8452_vector" {
    // The reverse direction: a ciphertext this implementation never produced
    // still opens to the standard's plaintext.
    let cipher = baml.crypto.Aes256GcmSiv.new(
        baml.Uint8Array.from_hex(
            "0100000000000000000000000000000000000000000000000000000000000000",
        ),
    );
    let opened = cipher.decrypt(
        baml.Uint8Array.from_hex("030000000000000000000000"),
        baml.Uint8Array.from_hex("1de22967237a813291213f267e3b452f02d01ae33e4ec854"),
        baml.Uint8Array.from_hex("01"),
    );
    assert.equal(opened.to_hex(), "0200000000000000")
}

// ─── Round trips ────────────────────────────────────────────────────────────

// Takes `Aead` rather than a concrete cipher on purpose: reaching both methods
// through an interface-typed parameter is what the test below is checking.
function seal_and_open(
    cipher: baml.crypto.Aead,
    nonce: uint8array,
    message: uint8array,
    aad: uint8array,
) -> bool {
    cipher.decrypt(nonce, cipher.encrypt(nonce, message, aad), aad) == message
}

test "aes256_round_trips_through_the_aead_interface" {
    // Key and nonce from the real system CSPRNG, and both directions reached
    // through an `Aead`-typed parameter so the `VirtualCall` resolves the impl
    // on the runtime class.
    let rng = baml.random.SystemRandom.get().as<baml.random.Rng>;
    let cipher = baml.crypto.Aes256GcmSiv.new(baml.crypto.Aes256GcmSiv.random_key(rng));
    assert.is_true(
        seal_and_open(
            cipher.as<baml.crypto.Aead>,
            rng.random(12),
            b"attack at dawn",
            b"envelope-v1",
        ),
    )
}

test "aes256_round_trips_empty_plaintext_and_aad" {
    // Nothing to encrypt still produces a bare 16-byte tag, which opens back to
    // empty.
    let cipher = baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(32));
    let nonce = baml.Uint8Array.zeroes(12);
    let empty = baml.Uint8Array.zeroes(0);
    let sealed = cipher.encrypt(nonce, empty, empty);
    assert.equal(sealed.length(), 16);
    assert.equal(cipher.decrypt(nonce, sealed, empty).length(), 0)
}

test "aes128_ciphertext_is_plaintext_plus_a_tag" {
    // The tag is a fixed 16-byte overhead, and `aad` is authenticated but not
    // encrypted, so it must not change the ciphertext's length.
    let cipher = baml.crypto.Aes128GcmSiv.new(baml.Uint8Array.zeroes(16));
    let nonce = baml.Uint8Array.zeroes(12);
    let message = b"sixteen bytes!!!";
    let no_aad = cipher.encrypt(nonce, message, baml.Uint8Array.zeroes(0));
    let long_aad = cipher.encrypt(nonce, message, baml.Uint8Array.zeroes(4096));
    assert.equal(no_aad.length(), message.length() + 16);
    assert.equal(long_aad.length(), no_aad.length())
}

// ─── Nonce misuse ───────────────────────────────────────────────────────────

test "aes256_repeated_nonce_leaks_only_equality" {
    // The SIV property: a repeated `(key, nonce)` pair reveals only whether two
    // messages were identical. Identical inputs give identical ciphertexts; a
    // changed plaintext or a changed aad does not.
    let cipher = baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(32));
    let nonce = baml.Uint8Array.zeroes(12);
    let aad = b"ctx";
    let sealed = cipher.encrypt(nonce, b"same message", aad);
    assert.equal(cipher.encrypt(nonce, b"same message", aad), sealed);
    assert.is_true(cipher.encrypt(nonce, b"different!!!", aad) != sealed);
    assert.is_true(cipher.encrypt(nonce, b"same message", b"other-ctx") != sealed)
}

// ─── Rejected ciphertexts ───────────────────────────────────────────────────

test "aes256_wrong_aad_is_rejected" {
    let cipher = baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(32));
    let nonce = baml.Uint8Array.zeroes(12);
    let sealed = cipher.encrypt(nonce, b"secret", b"envelope-v1");
    {
        cipher.decrypt(nonce, sealed, b"envelope-v2");
        baml.sys.panic("expected decrypt under the wrong aad to throw DecryptionFailure")
    } catch (e) {
        baml.crypto.DecryptionFailure => {
            assert.equal(e.algorithm, "AES-256-GCM-SIV");
            assert.equal(e.reason, "authentication failed")
        }
    }
}

test "aes256_tampered_ciphertext_is_rejected" {
    // Flipping one bit of the ciphertext body must fail authentication rather
    // than yield a corrupted plaintext.
    let cipher = baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(32));
    let nonce = baml.Uint8Array.zeroes(12);
    let empty = baml.Uint8Array.zeroes(0);
    let bytes = cipher.encrypt(nonce, b"secret", empty).to_array();
    bytes[0] = bytes[0] ^ 1;
    let tampered = baml.Uint8Array.from_array(bytes);
    {
        cipher.decrypt(nonce, tampered, empty);
        baml.sys.panic("expected a tampered ciphertext to throw DecryptionFailure")
    } catch (e) {
        baml.crypto.DecryptionFailure => assert.equal(e.reason, "authentication failed")
    }
}

test "aes256_ciphertext_too_short_to_hold_a_tag_is_named" {
    // A buffer too short to contain a tag is a `DecryptionFailure`, since it is
    // ciphertext rather than a malformed call. Its reason names the length,
    // which the caller already chose, instead of the opaque tag-mismatch text.
    let cipher = baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(32));
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

// ─── Malformed calls ────────────────────────────────────────────────────────

test "aes256_wrong_key_length_is_invalid_argument" {
    {
        baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(31));
        baml.sys.panic("expected a 31-byte key to throw InvalidArgument")
    } catch (e) {
        baml.errors.InvalidArgument => assert.equal(
            e.message,
            "AES-256-GCM-SIV: key must be exactly 32 bytes, got 31",
        )
    }
}

test "aes128_oversized_key_is_rejected_not_truncated" {
    // A 32-byte key is an `Aes256GcmSiv` key and nothing else. Handing it to the
    // 128-bit cipher must fail rather than silently use its first 16 bytes,
    // which would produce ciphertexts no peer could reproduce.
    {
        baml.crypto.Aes128GcmSiv.new(baml.Uint8Array.zeroes(32));
        baml.sys.panic("expected a 32-byte key to throw InvalidArgument")
    } catch (e) {
        baml.errors.InvalidArgument => assert.equal(
            e.message,
            "AES-128-GCM-SIV: key must be exactly 16 bytes, got 32",
        )
    }
}

test "aes256_wrong_nonce_length_is_invalid_argument" {
    // A malformed call, not a rejected ciphertext, so it is `InvalidArgument`
    // even on the decrypt side, where a `DecryptionFailure` would wrongly
    // suggest the data was at fault.
    let cipher = baml.crypto.Aes256GcmSiv.new(baml.Uint8Array.zeroes(32));
    {
        cipher.decrypt(
            baml.Uint8Array.zeroes(16),
            baml.Uint8Array.zeroes(32),
            baml.Uint8Array.zeroes(0),
        );
        baml.sys.panic("expected a 16-byte nonce to throw InvalidArgument")
    } catch (e) {
        baml.errors.InvalidArgument => assert.equal(
            e.message,
            "AES-256-GCM-SIV: nonce must be exactly 12 bytes, got 16",
        )
    }
}

// ─── Key generation ─────────────────────────────────────────────────────────

test "random_key_draws_the_algorithms_key_length" {
    let rng = baml.random.SystemRandom.get().as<baml.random.Rng>;
    assert.equal(baml.crypto.Aes256GcmSiv.random_key(rng).length(), 32);
    assert.equal(baml.crypto.Aes128GcmSiv.random_key(rng).length(), 16)
}

test "random_key_feeds_new_directly" {
    // `GenerateKey` returns raw bytes so they can be stored or transported. The
    // contract is that the same algorithm's `new` accepts them as they are.
    let rng = baml.random.ChaCha20.new().as<baml.random.Rng>;
    let cipher = baml.crypto.Aes128GcmSiv.new(baml.crypto.Aes128GcmSiv.random_key(rng));
    let nonce = rng.random(12);
    assert.equal(
        cipher.decrypt(nonce, cipher.encrypt(nonce, b"round trip", b""), b""),
        b"round trip",
    )
}
