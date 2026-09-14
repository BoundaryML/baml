function from_later_annotation() -> int {
    let a = (b) -> {
        1
    };
    let c: (int) -> int throws never = a;
                                    // ^ (b: int) -> int throws never
    c(0)
}

function from_later_call() -> int {
    let a = (b) -> {
        1
    };
    a(0)
 // ^ (b: int) -> int throws never
}

function parameter_flows_to_return() -> int {
    let identity = (value) -> {
        value
    };
    let use_identity: (int) -> int throws never = identity;
                                               // ^^^^^^^^ (value: int) -> int throws never
    use_identity(1)
}
