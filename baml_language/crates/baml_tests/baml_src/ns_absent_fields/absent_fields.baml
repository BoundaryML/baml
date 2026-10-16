// An optional (`T?`) class field has three states: absent (never given a
// value), `null`, and a value. Reads cannot tell the first two apart;
// `reflect.class.is_field_set` and JSON encoding with `omit_absent = true` can.

class Patch {
    name: string?,
    expires_in: int?,
}

class Owner {
    id: int,
    patch: Patch?,
    patches: Patch[],
}

// ─── Construction ─────────────────────────────────────────────────────────────

test "a field the constructor leaves out is absent and reads as null" {
    let p = Patch { name: "x" };
    assert.equal(reflect.class.is_field_set(p, "name"), true);
    assert.equal(reflect.class.is_field_set(p, "expires_in"), false);
    assert.equal(p.expires_in, null)
}

test "a field written null is set" {
    let p = Patch { expires_in: null };
    assert.equal(reflect.class.is_field_set(p, "expires_in"), true);
    assert.equal(reflect.class.is_field_set(p, "name"), false);
    assert.equal(p.expires_in, null)
}

test "a required field is always set" {
    let o = Owner { id: 1, patches: [] };
    assert.equal(reflect.class.is_field_set(o, "id"), true);
    assert.equal(reflect.class.is_field_set(o, "patch"), false)
}

// ─── Encoding ─────────────────────────────────────────────────────────────────

test "by default an absent field is written as null" {
    assert.equal(baml.json.to_string(Patch { name: "x" }), "{\"name\":\"x\",\"expires_in\":null}")
}

test "omit_absent leaves absent fields out and keeps explicit nulls" {
    assert.equal(baml.json.to_string(Patch { name: "x" }, omit_absent = true), "{\"name\":\"x\"}");
    assert.equal(
        baml.json.to_string(Patch { expires_in: null }, omit_absent = true),
        "{\"expires_in\":null}",
    );
    assert.equal(baml.json.to_string(Patch {}, omit_absent = true), "{}")
}

test "omit_absent reaches nested classes, lists, and optionals" {
    let o = Owner { id: 1, patch: Patch { name: "a" }, patches: [Patch { expires_in: 3 }, Patch {}] };
    assert.equal(
        baml.json.to_string(o, omit_absent = true),
        "{\"id\":1,\"patch\":{\"name\":\"a\"},\"patches\":[{\"expires_in\":3},{}]}",
    )
}

test "from and to_json take omit_absent too" {
    let p = Patch { name: "x" };
    assert.equal(baml.json.from(p, omit_absent = true), { "name": "x" });
    assert.equal(baml.json.from(p), { "name": "x", "expires_in": null });
    assert.equal(baml.json.to_json(p, omit_absent = true), { "name": "x" })
}

// ─── Decoding ─────────────────────────────────────────────────────────────────

test "from_string keeps a missing member apart from an explicit null" {
    let p = baml.json.from_string<Patch>("{\"expires_in\":null}");
    assert.equal(reflect.class.is_field_set(p, "name"), false);
    assert.equal(reflect.class.is_field_set(p, "expires_in"), true);
    assert.equal(p.name, null);
    assert.equal(p.expires_in, null)
}

test "from_string round-trips presence with omit_absent" {
    for (
        let text in [
            "{}",
            "{\"name\":null}",
            "{\"expires_in\":7}",
            "{\"name\":\"a\",\"expires_in\":null}",
        ]
    ) {
        let p = baml.json.from_string<Patch>(text);
        assert.equal(baml.json.to_string(p, omit_absent = true), text)
    }
}

test "to keeps a missing member apart from an explicit null at every depth" {
    let o = baml.json.to<Owner>(
        baml.json.parse("{\"id\":1,\"patch\":{\"name\":null},\"patches\":[{},{\"expires_in\":2}]}"),
    );
    assert.equal(
        baml.json.to_string(o, omit_absent = true),
        "{\"id\":1,\"patch\":{\"name\":null},\"patches\":[{},{\"expires_in\":2}]}",
    );
    let inner: Patch = o.patch else {
        throw "patch should decode";
    };
    assert.equal(reflect.class.is_field_set(inner, "expires_in"), false)
}

test "a missing optional class member decodes absent" {
    let o = baml.json.from_string<Owner>("{\"id\":1,\"patches\":[]}");
    assert.equal(reflect.class.is_field_set(o, "patch"), false);
    assert.equal(baml.json.to_string(o, omit_absent = true), "{\"id\":1,\"patches\":[]}");
    assert.equal(baml.json.to_string(o), "{\"id\":1,\"patch\":null,\"patches\":[]}")
}

// ─── Assignment and unset_field ───────────────────────────────────────────────

test "assigning a field sets it, even to null" {
    let p = Patch {};
    p.expires_in = null;
    assert.equal(reflect.class.is_field_set(p, "expires_in"), true);
    assert.equal(baml.json.to_string(p, omit_absent = true), "{\"expires_in\":null}")
}

test "unset_field makes a field absent again" {
    let p = Patch { name: "x", expires_in: 5 };
    reflect.class.unset_field(p, "expires_in");
    assert.equal(reflect.class.is_field_set(p, "expires_in"), false);
    assert.equal(p.expires_in, null);
    assert.equal(baml.json.to_string(p, omit_absent = true), "{\"name\":\"x\"}")
}

test "unset_field rejects a field that cannot be null" {
    let o = Owner { id: 1, patches: [] };
    let message = "";
    reflect.class.unset_field(o, "id") catch (e) {
        reflect.errors.CompilationError => {
            message = e.diagnostics[0].message
        },
    };
    assert.contains(message, "does not admit null")
}

test "presence checks reject unknown fields and non-instances" {
    let unknown_field = "";
    reflect.class.is_field_set(Patch {}, "nope") catch (e) {
        reflect.errors.CompilationError => {
            unknown_field = e.diagnostics[0].message
        },
    };
    assert.contains(unknown_field, "has no field `nope`");
    let not_instance = false;
    reflect.class.unset_field(3, "name") catch (_) {
        reflect.errors.CompilationError => {
            not_instance = true
        },
    };
    assert.equal(not_instance, true)
}

// ─── Values that ignore presence ──────────────────────────────────────────────

test "equality ignores presence" {
    assert.equal(baml.json.from_string<Patch>("{}") == Patch { name: null, expires_in: null }, true);
    assert.equal(Patch {} == Patch { expires_in: null }, true)
}

test "display renders an absent field as null" {
    assert.equal(string.from(Patch { name: "x" }), "Patch { name: \"x\", expires_in: null }")
}

// A spread copies each field as read, so an absent field of the base arrives
// as an explicit null. Pinned so a change to that is deliberate.
test "a spread copies an absent field as null" {
    let base = Patch { name: "x" };
    let copy = Patch { ...base };
    assert.equal(reflect.class.is_field_set(copy, "expires_in"), true)
}
