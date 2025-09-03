fn IfThenOnlyHeader() -> int {
    let x = 1;

    # Start
    let y = if (false) { 1 };

    ## After
    y
}


