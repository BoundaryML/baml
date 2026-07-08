class Order {
    id: string,
    created_epoch: int,
}

function age_days(o: Order, now_epoch: int) -> int {
    let delta = now_epoch - o.created_epoch;
    delta / 86400
}

test "age" {
    assert.equal(age_days(Order { id: "x", created_epoch: 0 }, 86400), 1);
}
