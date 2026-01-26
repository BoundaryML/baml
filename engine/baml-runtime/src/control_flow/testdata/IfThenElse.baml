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
    x + 4
}
