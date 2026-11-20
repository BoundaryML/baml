interface I requires A, B, // c
{}

interface J requires A, // c
{}

test "t" with a, b, // c
{
    1
}

function f() -> int {
    let h = spawn with a, b // c
    , { 1 };
    let i = [
        - // c8
        x // c11
    ];
    1
}

function g(flag: bool // c3
?// c
) -> int {
    1
}
