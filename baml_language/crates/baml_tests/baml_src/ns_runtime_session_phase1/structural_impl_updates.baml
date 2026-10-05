test "earlier_session_body_observes_later_structural_override" {
    let session = reflect.Session.new()
    session.eval(`
        class Value { n int }
        function render(v: Value) -> string { v.to_string() }
        function encode(v: Value) -> baml.json.json { v.to_json() }
    `)
    assert.equal(session.eval<bool>(`render(Value { n: 1 }) == "updated"`), false)
    assert.equal(session.eval<bool>(`encode(Value { n: 1 }) == "updated"`), false)
    session.eval(`
        implements baml.ToString for Value {
            function to_string(self) -> string { "updated" }
        }
        implements baml.ToJson for Value {
            function to_json(self) -> baml.json.json throws baml.json.SerializationError {
                "updated"
            }
        }
    `)
    assert.equal(session.eval<string>(`render(Value { n: 1 })`), "updated")
    assert.equal(session.eval<bool>(`encode(Value { n: 1 }) == "updated"`), true)
}

test "earlier_session_equality_observes_later_structural_override" {
    let session = reflect.Session.new()
    session.eval(`
        class Value { n int }
        function equal(a: Value, b: Value) -> bool { a == b }
    `)
    assert.equal(session.eval<bool>(`equal(Value { n: 1 }, Value { n: 2 })`), false)
    session.eval(`
        implements baml.ops.Equals for Value {
            function eq(self, other: Self) -> bool { true }
        }
    `)
    assert.equal(session.eval<bool>(`equal(Value { n: 1 }, Value { n: 2 })`), true)
}

test "earlier_session_decoder_observes_later_structural_override" {
    let session = reflect.Session.new()
    session.eval(`
        class Value { n int }
        function decode() -> int { Value.from_json((Value { n: 1 }).to_json()).n }
        function decode_bound() -> int {
            let decode = Value.from_json;
            decode((Value { n: 1 }).to_json()).n
        }
    `)
    assert.equal(session.eval<int>(`decode()`), 1)
    assert.equal(session.eval<int>(`decode_bound()`), 1)
    session.eval(`
        implements baml.FromJson for Value {
            function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
                Value { n: 42 }
            }
        }
    `)
    assert.equal(session.eval<int>(`decode()`), 42)
    assert.equal(session.eval<int>(`decode_bound()`), 42)
}

test "earlier_session_hash_observes_later_structural_override" {
    let session = reflect.Session.new()
    session.eval(`
        class Value { n int }
        enum Choice { A B }
        function hash_class() -> int {
            let state = baml.hash.DefaultHasher.new();
            (Value { n: 1 }).hash(state)
            state.finish()
        }
        function hash_enum(value: Choice) -> int {
            let state = baml.hash.DefaultHasher.new();
            value.hash(state)
            state.finish()
        }
        function hash_variant() -> int {
            let state = baml.hash.DefaultHasher.new();
            let value: Choice.A = Choice.A;
            value.hash(state)
            state.finish()
        }
        function expected() -> int {
            let state = baml.hash.DefaultHasher.new();
            "updated".hash(state)
            state.finish()
        }
    `)
    assert.equal(session.eval<bool>(`hash_class() == expected()`), false)
    assert.equal(session.eval<bool>(`hash_enum(Choice.A) == expected()`), false)
    assert.equal(session.eval<bool>(`hash_variant() == expected()`), false)
    session.eval(`
        implements baml.Hash for Value {
            function hash(self, state: baml.hash.Hasher) -> void throws never {
                "updated".hash(state)
            }
        }
        implements baml.Hash for Choice {
            function hash(self, state: baml.hash.Hasher) -> void throws never {
                "updated".hash(state)
            }
        }
    `)
    assert.equal(session.eval<bool>(`hash_class() == expected()`), true)
    assert.equal(session.eval<bool>(`hash_enum(Choice.A) == expected()`), true)
    assert.equal(session.eval<bool>(`hash_variant() == expected()`), true)
}
