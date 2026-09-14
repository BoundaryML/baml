//baml:tagged_string
function chat(body: (role: string) -> baml.TaggedString) -> string {
    "ok"
}

function Demo() -> string {
    chat`hello ${role}`
}

test "tagged_interp_resolves_body_lambda_param" {
    assert.equal(1, 1)
}
