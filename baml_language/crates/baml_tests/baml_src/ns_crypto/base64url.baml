test "base64url_known_vectors" {
    assert.equal(b"".to_base64url(), "");
    assert.equal(b"f".to_base64url(), "Zg");
    assert.equal(b"fo".to_base64url(), "Zm8");
    assert.equal(b"foo".to_base64url(), "Zm9v");
    let binary = uint8array.from_hex("fbffff");
    assert.equal(binary.to_base64url(), "-___");
    assert.equal(binary.to_base64(), "+///");
    assert.equal(uint8array.from_base64url("-___"), binary);
    assert.equal(uint8array.from_base64url(""), b"");
    assert.equal(uint8array.from_base64url("Zg"), b"f");
    assert.equal(uint8array.from_base64url("Zm8"), b"fo");
    assert.equal(uint8array.from_base64url("Zm9v"), b"foo")
}

test "base64url_rejects_noncanonical_input" {
    for (let text in ["Zg=", "Zg==", "Zg\n", "Z g", "+///", "A", "Zh", "Zm9", "é"]) {
        {
            uint8array.from_base64url(text);
            baml.sys.panic("expected InvalidArgument")
        } catch (e) {
            baml.errors.InvalidArgument => assert.equal(e.message, "invalid unpadded base64url")
        }
    }
}
