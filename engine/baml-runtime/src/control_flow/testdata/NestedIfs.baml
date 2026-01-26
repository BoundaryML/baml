//# Main Function
function NestedIfs() -> int {
    //## If statement 1
    let e = if (true) {

        //### If statement 2.1
        let z = if (true) {
            //### If statement 3.1
            let a = if (true) {
                //### True
                12
            } else {
                //### Third False
                13
            };

            242
        } else {
            //### If statement 3.2
            let a = if (true) {
                //### True
                12
            } else {
                //### Third False
                13
            };
            13
        };

        2
    } else {
        let z = if (true) {
            let a = if (true) {
                12
            } else if (false) {
                //### Second False
                14
            } else {
                13
            };

            242
        } else if (false) {
            if (true) {
                //# standalone then
                let standaloneThen = 18;
            }
            15
        } else{
            let a = if (true) {
                12
            } else {
                13
            };
            13
        };
        13
    };

    e
}
