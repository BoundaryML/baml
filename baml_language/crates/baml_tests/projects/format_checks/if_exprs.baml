// Testing all the formatter cases for if expressions

function MyFunction(a: int, b: int) -> int {

    if (a > b) {
        1;
    } else {
        1;
    }

    // 1 if leading
    if // 2 if trailing
    // 3 condition leading
    (a > b) // 4 condition trailing
    // 5 then leading
    { // 6 then inside
        1;
    } // 7 then trailing
    // 8 else leading
    else // 9 else trailing
    // 10 else then leading
    { // 11 else then inside
        1;
    } // 12 else then trailing

    ///////////////////

    /* 1 if leading */
    if /* 2 if trailing */
    /* 3 condition leading */
    (a > b) /* 4 condition trailing */
    /* 5 then leading */
    { /* 6 then inside */
        1;
    } /* 7 then trailing */
    /* 8 else leading */
    else /* 9 else trailing */
    /* 10 else then leading */
    { /* 11 else then inside */
        1;
    } /* 12 else then trailing */

    1
}