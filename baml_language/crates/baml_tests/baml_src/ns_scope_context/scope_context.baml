function leaf() -> int { 7 }

test "scope context accepts primitive metadata and chained capture" {
    let id = boundary.id()
        .context(metadata = { "text": "a", "count": 1, "ratio": 1.5, "flag": true, "gone": null })
        .capture(inputs = true)
        .context(metadata = { "count": 2 }, distinct_id = "user")
        .capture(output = false)
        .context();
    assert.equal(leaf($id = id), 7)
}

test "scope context keeps local ids single use" {
    let id = boundary.id().context(distinct_id = "user");
    leaf($id = id);
    let rejected = leaf($id = id) catch (e) {
        baml.errors.InvalidArgument => 42
    };
    assert.equal(rejected, 42)
}

test "scope context cannot edit a consumed id" {
    let id = boundary.id().context();
    leaf($id = id);
    let rejected = {
        id.context(metadata = { "later": true });
        false
    } catch (e) {
        baml.errors.InvalidArgument => true
    };
    assert.equal(rejected, true)
}

test "scope context works on dynamic and immediate calls" {
    let dynamic = leaf;
    assert.equal(dynamic($id = boundary.id().context(metadata = { "dynamic": true })), 7);
    assert.equal((() -> { 8 })($id = boundary.id().context(metadata = { "immediate": true })), 8)
}
