//baml:tagged_string
function sql(body: (x: int) -> baml.TaggedString) -> string {
    "ok"
}

function Demo() -> string {
    let q = sql`SELECT ${1}`;
    q
}

test "tagged_template_as_let_initializer_survives_front_end" {
    assert.equal(1, 1)
}
