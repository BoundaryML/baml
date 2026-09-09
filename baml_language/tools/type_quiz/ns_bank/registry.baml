// Every item in the bank, in one place. The conformance suite checks by
// reflection that no `Item` implementor in this package is missing here.

function items() -> root.engine.Item[] throws never {
    [UnionNotAssignableToMember {  }]
}
