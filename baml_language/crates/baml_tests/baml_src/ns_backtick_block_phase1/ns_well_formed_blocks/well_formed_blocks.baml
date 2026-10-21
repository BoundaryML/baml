function G() -> string {
    client: "openai/gpt-4o-mini"
    prompt: `${for (let x in [1, 2])}${x}${endfor}${if (true)}a${else if (false)}b${else}c${endif}`
}

test "well_formed_blocks_have_no_structural_diagnostic" {
    // Reaching the test means the complete for/if block structure compiled.
    assert.equal(1, 1)
}
