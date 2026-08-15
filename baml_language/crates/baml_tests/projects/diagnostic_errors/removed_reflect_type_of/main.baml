// BEP-066: the removed `type.of` reader is an ordinary nonexistent
// function. Both the bare shorthand spelling and the fully qualified one must
// produce an ordinary unresolved-name diagnostic.

function old_bare_spelling() -> type {
    type.of<int>()
}

function old_qualified_spelling() -> type {
    baml.type.of<string>()
}
