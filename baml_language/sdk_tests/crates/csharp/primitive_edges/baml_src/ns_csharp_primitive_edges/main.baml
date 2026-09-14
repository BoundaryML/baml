function int_min() -> int {
    -4611686018427387904
}

function int_max() -> int {
    4611686018427387903
}

function echo_int(value: int) -> int {
    value
}

function reject_int_before_dispatch(probe: () -> int, value: int) -> int {
    probe()
    value
}

function echo_bigint(value: bigint) -> bigint {
    value
}

function echo_float(value: float) -> float {
    value
}

function reject_float_before_dispatch(probe: () -> int, value: float) -> float {
    probe()
    value
}

function echo_bytes(value: uint8array) -> uint8array {
    value
}

function echo_nullable_int(value: int?) -> int? {
    value
}

function echo_nullable_string(value: string?) -> string? {
    value
}

function echo_nullable_bytes(value: uint8array?) -> uint8array? {
    value
}

function nullable_bytes_is_null(value: uint8array?) -> bool {
    value == null
}

function return_null_bytes() -> uint8array? {
    null
}

function echo_nullable_list(value: int[]?) -> int[]? {
    value
}

function echo_list_of_nullable_ints(value: int?[]) -> int?[] {
    value
}

function echo_nullable_map(value: map<string, string>?) -> map<string, string>? {
    value
}

function generic_maybe<T>(value: T?) -> T? {
    value
}
