// docs:start all
function main() -> int {
    let remaining = 8_000;
    log.info(remaining);

    remaining = remaining - 512;
    remaining = remaining - 3_100;
    log.info(remaining);

    remaining
}
// docs:end all

test "chapter example" {
    assert.equal(main(), 4388)
}
