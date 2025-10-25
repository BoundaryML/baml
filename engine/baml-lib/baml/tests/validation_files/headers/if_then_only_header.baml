//# Case 1
function IfThenOnlyHeader() -> int {
    let x = 1;

    //# Before 

    //# If
    let y = if (false) { 1 };

    //## After
    y
}

//# Case 2
function IfThenOnlyHeader2() -> int {
    let x = 1;
    //## Before

    let z = 5;

    //### If
    let y = if (false) { 
        //## Then
        1
    };

    //## After
    y
}


//# Case 3
function IfThenOnlyHeader2() -> int {
    let x = 1;
    //### Before

    let z = 5;

    //# If
    let y = if (false) { 
        //## Then
        1
    };

    //## After
    y
}
