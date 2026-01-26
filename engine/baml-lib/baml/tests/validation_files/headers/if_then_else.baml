//# Case 1
function IfThenElse() -> int {
    let x = 1;

    //# Before 

    //# If should always do else
    if (false) { 
        //## impossible: return 1
        1
    } else {
        //## should always be 5
        5
    }

    //# After
    y
}


//# Main Function
function ComplexHeaderTest(x: int) -> int {
    let z = 5;
    //# oopsie

    //# standalone will render

    //# will not render
    let k = [1, 2, 3];
    //# will not render
    5;
    //# will not render
    true;
    //# will not render
    k[0];

    //# increment maybe?
    if (x < 10) {
        //## increment x
        x = x + 10;
    } else {
        //## dummy variable y
        let y = 5;
    }
    
    //# another increm
    x + 1234
}