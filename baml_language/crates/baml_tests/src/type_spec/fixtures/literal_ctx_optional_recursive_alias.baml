// A fresh aggregate literal is constructed at the expected type, even
// when that type hides its list/map arm behind an optional spelling of a
// recursive alias. `T?` is `T | null`; `json?` and `json` denote the same
// type, so their literals must agree (TYPE_SYSTEM.md: aliases and
// recursive types). Not JSON-specific: any recursive alias behaves alike.
type LcTree = int | LcTree[]

class LcTreeHolder {
    data LcTree?
}

function tree_field() -> LcTreeHolder throws never {
    LcTreeHolder { data: [[1]] }
                      // ^^^^^ user.LcTree[]
}

function tree_direct() -> LcTree throws never {
    [[1]]
//  ^^^^^ user.LcTree[]
}

class LcJsonHolder {
    data baml.json.json?
}

function json_field() -> LcJsonHolder throws never {
    LcJsonHolder { data: {key: {nested: false}} }
                      // ^^^^^^^^^^^^^^^^^^^^^^ map<string, baml.json.json>
}
