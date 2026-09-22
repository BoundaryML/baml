// docs:start all
function main() -> string {
    let tokens = 950_000;
    let price_cents_per_mtok = 300;

    let cents = tokens * price_cents_per_mtok / 1_000_000;
    let dollars = cents / 100;

    `this call will cost about $${dollars}`
}
// docs:end all

test "chapter example" {
    assert.equal(main(), "this call will cost about $2")
}
