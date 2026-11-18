test "tuple_equality_and_ordering_keep_inline_and_boxed_float_semantics" {
    let finite = (1.5,);
    let nan = (float.nan(),);
    assert.is_true(finite < nan);
    assert.is_true(nan > finite);
    assert.equal(nan, (float.nan(),));
    assert.equal((0.0,), (-0.0,));
    assert.is_true([(1.5,), (float.nan(),)].includes((float.nan(),)))
}

test "tuple_decoding_reuses_warm_lookups_after_failures_and_collection" {
    for (let round = 0; round < 3; round += 1) {
        assert.equal(baml.json.decode<(int, TpjOverride)>(`(1, {"n": 1})`).1.n, 42);
        let missing = baml.json.decode<(int, TpjOverride)>(`(1,)`) catch (e) {
            baml.json.DecodeError => null,
        };
        assert.equal(missing, null);
        let malformed = baml.json.decode<(int, TpjOverride)>(`(1,]`) catch (e) {
            baml.json.ParseError => null,
        };
        assert.equal(malformed, null);
        baml.sys.collect_garbage();
        assert.equal(baml.json.decode<(int, TpjOverride)>(`[2, {"n": 2}]`).1.n, 42)
    }
}
