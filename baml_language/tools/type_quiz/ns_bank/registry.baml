// Every item in the bank, in one place. The conformance suite checks by
// reflection that every fact generator in this package is registered in a
// shape of `shapes()`, and every shape becomes an item here.

function items() -> root.engine.Item[] throws never {
    let flows: root.engine.Item[] = shapes().map((shape) -> {
        FlowItem { shape, status: root.engine.Verified {  }, sites: root.algebra.all_sites() }
    });
    flows.concat([UnguardedCycleItem {  }, NestedUnionLetItem {  }])
}
