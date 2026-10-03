// `await f catch (e) {…}` must parse as `(await f) catch …` so the handler
// catches the error `await` re-throws from the future (regression test for
// the await/catch precedence fix).

function bad() -> int throws string {
    throw "boom"
}

function main() -> int {
    let f = spawn { bad() };
    await f catch (e) {
        let e => 7
    }
}

test "await_catch_binds_to_await_not_future" {
    assert.equal(main(), 7)
}
