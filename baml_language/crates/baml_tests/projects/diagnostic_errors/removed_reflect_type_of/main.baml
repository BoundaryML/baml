// BEP-066: the removed `reflect.type_of` reader is an ordinary nonexistent
// function. Both the bare shorthand spelling and the fully qualified one must
// produce an ordinary unresolved-name diagnostic.

function old_bare_spelling() -> reflect.Type {
    reflect.type_of<int>()
}

function old_qualified_spelling() -> reflect.Type {
    reflect.type_of<string>()
}
