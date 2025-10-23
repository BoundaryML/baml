
//# Main Function
function ComplexHeaderTest(x: int) -> int {
    //# increment by 10
    if (x < 10) {
        x = x + 10;
    } else {
        //## dummy variable y
        let y = 5;
    }
    
    //# another increment
    x + 1234
}

test Foo {
    functions [ComplexHeaderTest]
    args {
        x 5
    }
}