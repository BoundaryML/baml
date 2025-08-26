// --------------------------------
// Pure Expression Functions.
// --------------------------------

function ReturnOne() -> int {
    1
}

function ReturnNumber(n: int) -> int {
    n
}

function CallReturnOne() -> int {
    ReturnOne()
}

function ChainedCalls() -> int {
    ReturnNumber(CallReturnOne())
}

function StoreFnCallInLocalVar(n: int) -> int {
    let result = ReturnNumber(n);

    result
}

function BoolToIntWithIfElse(b: bool) -> int {
    if (b) { 1 } else { 0 }
}

function ReturnElseIfExpr(a: bool, b: bool) -> int {
    if (a) {
        1
    } else if (b) {
        2
    } else {
        3
    }
}

function AssignElseIfExpr(a: bool, b: bool) -> int {
    let result = if (a) {
        1
    } else if (b) {
        2
    } else {
        3
    };

    result
}

function NormalElseIfStmt(a: bool, b: bool) -> int {
    let v = 0;

    if (a) {
        let one = 1;
        StoreFnCallInLocalVar(one);
    } else if (b) {
        let two = 2;
        StoreFnCallInLocalVar(two);
    } else {
        let three = 3;
        StoreFnCallInLocalVar(three);
    }

    v
}

function IterativeFibonacci(n: int) -> int {
    let mut a = 0;
    let mut b = 1;

    if (n == 0) {
        b
    } else {
        let mut i = 1;
        while (i <= n) {
            let c = a + b;
            a = b;
            b = c;
            i += 1;
        }
        a
    }
}

function SumArray(arr: int[]) -> int {
    let mut sum = 0;
    for (a in arr) {
        sum += a;
    }
    sum
}

function SumFromTo(x: int, y: int) -> int {
    let mut s = 0;

    for (let mut i = x; i <= y; i += 1) {
        s += i;
    }

    s
}

// --------------------------------
// Expression Functions with LLM calls.
// --------------------------------

function LlmReturnNumber(n: int) -> int {
    client GPT35LegacyProvider // GPT 3.5 Turbo
    prompt #"
        Return the number {{ n }} without any additional text.
    "#
}

function ReturnNumberCallingLlm(n: int) -> int {
    LlmReturnNumber(n)
}

function StoreLlmCallInLocalVar(n: int) -> int {
    let result = LlmReturnNumber(n);

    result
}

function BoolToIntWithIfElseCallingLlm(b: bool) -> int {
    if (b) { LlmReturnNumber(1) } else { LlmReturnNumber(0) }
}
