// Registry cycle detection: `implement<T extends Loop> Loop for T` -
// resolving `C: Loop` re-enters the same goal through its own bound
// (goal -> match T := C -> bound `C: Loop` -> goal). The in-progress
// goal stack rejects the repeat immediately (rustc's obligation-stack
// cycle rule: a goal on its own proof path has no finite proof), where
// the depth budget alone burned 16 levels first. No absorption: the
// join keeps both members.
interface Loop {}

implement<T extends Loop> Loop for T {}

class C {
    x int
}

function f(c: C, l: Loop, b: bool) -> unknown throws never {
    let u = if (b) { c } else { l };
     // ^ user.C | user.Loop
    u
}
