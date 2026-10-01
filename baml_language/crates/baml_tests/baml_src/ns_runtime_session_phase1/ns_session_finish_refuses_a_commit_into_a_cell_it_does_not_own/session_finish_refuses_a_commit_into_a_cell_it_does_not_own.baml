// An assignment step commits into the visible binding it names. A submission
// compiled on one session names that session's cell; finishing it on a
// session that owns no such cell is a link error, never a silent commit into
// the step's own slot. The refused submission publishes nothing: the target
// session keeps its bindings and takes further submissions.
function main() -> string {
    let a = reflect.Session.new();
    a.eval(`let n = 5`);
    let b = reflect.Session.new();
    b.eval(`let m = 1`);
    let artifact = a._compile<unknown>(`n = 9`);
    let refused = "accepted";
    let _ = b._finish<unknown>(artifact) catch (e) {
        baml.errors.InvalidArgument => {
            refused = "refused"
        },
        _ => {
            refused = "wrong error"
        },
    };
    let kept = b.eval<int>(`m`);
    let next = b.eval<int>(`let k = m + 1
k`);
    let a_kept = a.eval<int>(`n`);
    `${refused}|${kept}|${next}|${a_kept}`
}

test "session_finish_refuses_a_commit_into_a_cell_it_does_not_own" {
    assert.equal(main(), "refused|1|2|5")
}
