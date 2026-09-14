//baml:tagged_string
function sql(body: (x: int) -> baml.TaggedString) -> string {
    "ok"
}

function Demo(items: int[]) -> string {
    sql`SELECT ${1} ${for (let x in items)}col_${x}, ${endfor}${if (true)}WHERE 1${else}ALL${endif}`
}

test "tagged_template_with_interp_for_and_if_survives_front_end" {
    assert.equal(1, 1)
}
