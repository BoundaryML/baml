function TracedNativeLength() -> int {
    let values = [1, 2, 3];
    values.length($trace = trace.span())
}

function TracedNativeTiming() -> int {
    let values = [1, 2, 3];
    let length = values.length;
    length($trace = trace.timing())
}

function TracedNativeCallbacks() -> int[] {
    let values = [1, 2, 3];
    values.map(
        (x: int) -> int {
            x * 2
        },
        $trace = trace.span(),
    )
}

function TracedNativeCallbackError() -> int[] | string {
    let values = [1, 2, 3];
    values.map(
        (x: int) -> int {
            if (x == 2) {
                throw "callback failure"
            }
            x
        },
        $trace = trace.span(),
    ) catch (e) {
        _ => "caught"
    }
}

function TracedSysOp() -> int {
    baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n), $trace = trace.span());
    let sleep = baml.sys.sleep;
    sleep(baml.time.Duration.from_nanoseconds(0n), $trace = trace.timing());
    7
}

test "native direct span" {
    assert.equal(TracedNativeLength(), 3)
}

test "native bound timing" {
    assert.equal(TracedNativeTiming(), 3)
}

test "native resumed callbacks" {
    assert.equal(TracedNativeCallbacks(), [2, 4, 6])
}

test "native callback unwind" {
    assert.equal(TracedNativeCallbackError(), "caught")
}

test "sysop direct and indirect" {
    assert.equal(TracedSysOp(), 7)
}
