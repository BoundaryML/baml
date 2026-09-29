// Void tail discard + literal-typed impl goals
//
// A `-> void` function's tail must itself be void (E0176), so a value is
// discarded with `;` (explicit `return v` is checked too). And a literal-typed
// value implements what its base primitive does: `1` proves
// `LiGoal<int>` through `implements LiGoal<int> for int`.
interface LiGoal<T> {
    function tag(self) -> string throws never
}

implements LiGoal<int> for int {
    function tag(self) -> string throws never {
        "int"
    }
}

function vd_log(log: string[]) -> void throws never {
    log.push("body");
}

function li_child() -> string throws never {
    let child: LiGoal<int> = 1;
     // ^^^^^ user.LiGoal<int>
    child.tag()
}
