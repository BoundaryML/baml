function BranchArmsContainSequentialHeaders() -> int {
    //# Before
    let k = 0;
    let value = if (k == 1) {
        //# k == 1 BranchArm first
        //# k == 1 BranchArm second
        1
    } else if (k == 2) {
        //### k == 2 BranchArm first
        //### k == 2 BranchArm second
        //### k == 2 BranchArm third 
        2
    } else if (k == 3) {
        2
    } else {
        //## Else Branch second
        3
    };

    //# After

    value
}
