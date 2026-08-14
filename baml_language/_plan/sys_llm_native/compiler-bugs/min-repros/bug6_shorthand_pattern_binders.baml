// BUG 6 (post-hir_ty regression, two halves):
// (a) a QUOTED key whose text equals the value identifier was treated as
//     property shorthand; (b) shorthand's scope check could not see pattern
//     binders (if let / match / for / catch) or nested-block lets.
// Pre-fix: E0003 "property shorthand `key` requires an in-scope value named
// `key`" at CHECK time on both functions.
function bug6_quoted(v: string?) -> string {
    if let key: string = v {
        let params: map<string, string> = { "key": key };
        params.get("key") ?? ""
    } else {
        ""
    }
}

function bug6_shorthand(v: string?) -> string {
    if let name: string = v {
        let m: map<string, string> = { name };
        m.get("name") ?? ""
    } else {
        ""
    }
}

test "bug6_binders_visible_to_shorthand" {
    assert.equal(bug6_quoted("a"), "a");
    assert.equal(bug6_shorthand("b"), "b")
}
