// `==` on a `@@rust_type` value compares its value where the kind has one
// (media, a regex, a csv record), and its identity where it has none (a
// hasher's state, a host's value).

function first_record(text: string) -> baml.csv.Record? {
    let r = baml.csv.reader(text);
    let next = r.next() catch (_) {
        _ => {
            return null;
        },
    };
    match (next) {
        baml.iter.Done => null,
        let rec: baml.csv.Record => rec,
    }
}

test "media compare by value" {
    let a = baml.media.Image.from_url("https://example.com/cat.png", "image/png");
    let b = baml.media.Image.from_url("https://example.com/cat.png", "image/png");
    assert.is_true(a == b);
    assert.is_true(baml.deep_copy(a) == a);
    assert.is_true(a != baml.media.Image.from_url("https://example.com/dog.png", "image/png"));
    assert.is_true(a != baml.media.Image.from_url("https://example.com/cat.png", "image/jpeg"));
    assert.is_true(
        baml.media.Image.from_base64("YWJj", "image/png") == baml.media.Image.from_base64("YWJj", "image/png")
    );
    assert.is_true(
        baml.media.Image.from_base64("YWJj", "image/png") != baml.media.Image.from_base64("YWJk", "image/png")
    );
}

test "a regex compares by pattern and dialect" {
    let a = baml.regex.new("a+");
    assert.is_true(a == baml.regex.new("a+"));
    assert.is_true(baml.deep_copy(a) == a);
    assert.is_true(a != baml.regex.new("b+"));
    assert.is_true(a != baml.regex.new("a+", backtracking = true));
}

test "a csv record compares by value" {
    let a = first_record("x,y\n1,2\n");
    let b = first_record("x,y\n1,2\n");
    assert.is_true(a != null && a == b);
    assert.is_true(a != first_record("x,y\n1,3\n"));
    // The position a record was read at is part of it.
    assert.is_true(a != first_record("x,y\n\n1,2\n"));
}

test "a kind with no value apart from its state compares by identity" {
    let h = baml.hash.DefaultHasher.new();
    assert.is_true(h == h);
    assert.is_true(h != baml.hash.DefaultHasher.new());
    assert.is_true(baml.deep_copy(h) != h);
}
