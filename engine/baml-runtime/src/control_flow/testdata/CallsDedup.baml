//# Pipeline
function CallsDedup() -> int {
    //## Step A
    let a = Foo();

    //## Step B
    let b = Foo();

    //## Step C
    let c = Foo();

    a + b
}

function Foo() -> int {
    1
}
