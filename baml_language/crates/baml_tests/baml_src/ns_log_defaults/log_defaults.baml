function record(order: string[], value: string) -> string {
    order.push(value)
    value
}

function log_defaults_and_named_argument_order() -> string[] {
    let order: string[] = []
    log.info(record(order, "omitted"))
    log.debug(record(order, "null"), event_name = null)
    log.warn(
        event_name = record(order, "name-first"),
        data = record(order, "data-second"),
    )
    log.error(
        data = record(order, "data-first"),
        event_name = record(order, "name-second"),
    )
    order
}

test "log_defaults_and_named_argument_order" {
    assert.equal(log_defaults_and_named_argument_order(), [
        "omitted",
        "null",
        "name-first",
        "data-second",
        "data-first",
        "name-second",
    ])
}
