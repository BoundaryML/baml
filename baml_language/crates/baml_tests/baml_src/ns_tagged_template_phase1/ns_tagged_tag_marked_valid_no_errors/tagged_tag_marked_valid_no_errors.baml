//baml:tagged_string
function sql(body: (x: int) -> baml.TaggedString) -> string {
    "ok"
}

function Demo() -> string {
    sql`hi ${1}`
}

test "tagged_tag_marked_valid_no_errors" {
    assert.equal(1, 1)
}
