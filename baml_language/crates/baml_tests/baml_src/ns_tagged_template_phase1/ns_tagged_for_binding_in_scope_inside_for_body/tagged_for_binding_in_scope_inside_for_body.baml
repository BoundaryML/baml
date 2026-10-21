//baml:tagged_string
function chat(body: (role: string) -> baml.TaggedString) -> string {
    "ok"
}

function Demo(items: int[]) -> string {
    chat`${for (let x in items)}col_${x}, ${endfor}`
}

test "tagged_for_binding_in_scope_inside_for_body" {
    assert.equal(1, 1)
}
