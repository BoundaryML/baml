// Key-free tests for `baml.crypto.RsaPrivateKey` / `RsaPublicKey`. Signing
// round-trips against generated keys live in the Rust unit tests, so no key
// material is checked in here.

test "rs256_invalid_private_pem" {
    {
        baml.crypto.RsaPrivateKey.from_pem("not a key");
        baml.sys.panic("expected InvalidArgument")
    } catch (e) {
        baml.errors.InvalidArgument => assert.equal(e.message, "invalid or unsupported RSA key")
    }
}

test "rs256_invalid_public_pem" {
    {
        baml.crypto.RsaPublicKey.from_pem("-----BEGIN PUBLIC KEY-----\nAAAA\n-----END PUBLIC KEY-----");
        baml.sys.panic("expected InvalidArgument")
    } catch (e) {
        baml.errors.InvalidArgument => assert.equal(e.message, "invalid or unsupported RSA key")
    }
}

test "rs256_invalid_public_components" {
    for (let pair in [[b"", b"\x01\x00\x01"], [b"\x00", b"\x01\x00\x01"], [uint8array.from_hex("ff".repeat(256)), b"\x03"]]) {
        {
            baml.crypto.RsaPublicKey.from_components(pair[0], pair[1]);
            baml.sys.panic("expected InvalidArgument")
        } catch (e) {
            baml.errors.InvalidArgument => assert.equal(e.message, "invalid or unsupported RSA key")
        }
    }
}

test "rs256_verify_rejects_garbage" {
    // A well-formed but meaningless modulus: 2048 bits, odd. No signature can
    // verify under it, which is enough to pin the false paths.
    let key = baml.crypto.RsaPublicKey.from_components(uint8array.from_hex("ff".repeat(256)), b"\x01\x00\x01");
    assert.is_true(!key.verify_rs256(b"message", b""));
    assert.is_true(!key.verify_rs256(b"message", b"short"));
    assert.is_true(!key.verify_rs256(b"message", uint8array.from_hex("00".repeat(256))));
    assert.is_true(!key.verify_rs256(b"message", uint8array.from_hex("01".repeat(257))))
}

// Application policy stays in the caller; the stdlib only signs bytes.
function github_app_jwt(key: baml.crypto.RsaPrivateKey, app_id: string, now: int) -> string {
    let header = baml.json.stringify({ "alg": "RS256", "typ": "JWT" }).to_utf8().to_base64url();
    let claims = baml.json.stringify({ "iat": now - 30, "exp": now + 540, "iss": app_id })
        .to_utf8()
        .to_base64url();
    let message = header + "." + claims;
    message + "." + key.sign_rs256(message.to_utf8()).to_base64url()
}
