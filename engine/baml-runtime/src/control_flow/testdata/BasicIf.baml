function BasicIf() -> int {
    let x = 1;

    if (true) {
        //# standalone then
        let a = 18;
    }

    if (x == 2) {
        //# if inside if-elseif-else
        let b = 18;
    } else if (x == 3) {
        //# elseif inside if-elseif-else
        let b = 19;
    } else {
        //# else inside if-elseif-else
        let b = 20;
    }

    //# This is a named statement
    let x = if (true) {
        //# Then Statement
        12
    } else {
        //# Else Statement
        13
    };

    5
}
