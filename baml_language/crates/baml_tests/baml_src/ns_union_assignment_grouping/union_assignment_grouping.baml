class Invoice {
    number: int,
}

function grouped(source: int | Invoice) -> int | string | Invoice {
    let value: (int | string) | Invoice = source;
    value
}

function flat(source: int | Invoice) -> int | string | Invoice {
    let value: int | string | Invoice = source;
    value
}

test "grouped union preserves Invoice" {
    let invoice = Invoice { number: 42 };
    let result = grouped(invoice);
    assert.equal(result is Invoice, true);
    assert.equal(result, invoice)
}

test "flat union preserves Invoice" {
    let invoice = Invoice { number: 42 };
    let result = flat(invoice);
    assert.equal(result is Invoice, true);
    assert.equal(result, invoice)
}
