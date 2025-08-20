fn While() -> int {
    while (true) {}

    while (true {
       let i = 0; // TODO: for some reason empty loop without both parens does not parse with PEST
    }

    while true) {

    }

    while true {
       let i = 0; // TODO: for some reason empty loop without both parens does not parse with PEST
    }
}


fn ForIterator(xs: int[]) -> int {

    for (x in xs) {}

    for (x in xs {
        let i = 0;// TODO: for some reason empty loop without both parens does not parse with PEST
    }

    for x in xs) {

    }

    for x in xs {
        let i = 0;// TODO: for some reason empty loop without both parens does not parse with PEST
    }
}

fn ForRegular() -> int {
    for (let mut i = 0; i < 3; i += 1) {}

    for let mut i = 0; i < 3; i += 1) {

    }

    for (let mut i = 0; i < 3; i += 1 {

    }

    for let mut i = 0; i < 3; i += 1 {

    }
}

fn If() -> int {

    if (true) {

    }

    if true) {

    }

    if (true {
        let i = 0;// TODO: for some reason empty if without both parens does not parse with PEST
    }

    if true {
    }

	if true {} {}
	if (true {} {}
}

fn FnCall() -> int {
    If()
}

// error: Error validating: expected closing parentheses for this opening parentheses
//   -->  parens.baml:4
//    | 
//  3 | 
//  4 |     while (true {
//    | 
// error: Error validating: expected opening parentheses for this closing parentheses
//   -->  parens.baml:8
//    | 
//  7 | 
//  8 |     while true) {
//    | 
// error: Error validating: expected parentheses around while loop
//   -->  parens.baml:12
//    | 
// 11 | 
// 12 |     while true {
//    | 
// error: Error validating: expected closing parentheses for this opening parentheses
//   -->  parens.baml:22
//    | 
// 21 | 
// 22 |     for (x in xs {
//    | 
// error: Error validating: expected opening parentheses for this closing parentheses
//   -->  parens.baml:26
//    | 
// 25 | 
// 26 |     for x in xs) {
//    | 
// error: Error validating: expected parentheses around for loop header
//   -->  parens.baml:30
//    | 
// 29 | 
// 30 |     for x in xs {
//    | 
// error: Error validating: expected opening parentheses for this closing parentheses
//   -->  parens.baml:38
//    | 
// 37 | 
// 38 |     for let mut i = 0; i < 3; i += 1) {
//    | 
// error: Error validating: expected closing parentheses for this opening parentheses
//   -->  parens.baml:42
//    | 
// 41 | 
// 42 |     for (let mut i = 0; i < 3; i += 1 {
//    | 
// error: Error validating: expected parentheses around for loop header
//   -->  parens.baml:46
//    | 
// 45 | 
// 46 |     for let mut i = 0; i < 3; i += 1 {
//    | 
// error: Error validating: expected opening parentheses for this closing parentheses
//   -->  parens.baml:57
//    | 
// 56 | 
// 57 |     if true) {
//    | 
// error: Error validating: expected closing parentheses for this opening parentheses
//   -->  parens.baml:61
//    | 
// 60 | 
// 61 |     if (true {
//    | 
// error: Error validating: expected parentheses around if expression
//   -->  parens.baml:65
//    | 
// 64 | 
// 65 |     if true {
//    | 
