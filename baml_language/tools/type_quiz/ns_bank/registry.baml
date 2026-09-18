// Every item in the bank, in one place. The conformance suite checks by
// reflection that every fact generator in this package is registered in
// `facts()`, and every fact becomes an item here.

function items() -> root.engine.Item[] throws never {
    let flows: root.engine.Item[] = facts().map((fact) -> {
        FlowItem { fact: fact, status: root.engine.Verified {  }, sites: root.algebra.all_sites() }
    });
    flows.concat([UnguardedCycleItem {  }, NestedUnionLetItem {  }])
}
