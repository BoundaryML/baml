// ── Method Calls + Call Chains ──────────────────────────────────

function MakeBuilder(n: int, s: string) -> Builder {
    return Builder { count: n, label: s };
}

// call function → access method on result
function BuilderCount(n: int) -> int {
    return MakeBuilder(n, "test").GetCount();
}

// chain: function result → field → pass to another function
function BuilderToAdd(n: int) -> int {
    let b = MakeBuilder(n, "hello");
    return Add2(b.count);
}

// ── Nested Container Access Through Calls ───────────────────────

function MakeContainer() -> Container {
    return Container {
        items: [10, 20, 30],
        metadata: {"count": 3, "max": 30},
    };
}

// function returns Container → access items[0]
function ContainerFirst() -> int {
    return MakeContainer().items[0];
}

// function returns Container → access metadata["count"]
function ContainerMeta() -> int {
    return MakeContainer().metadata["count"];
}

// deep: function → field → index → pass to function
function ContainerChain() -> int {
    let c = MakeContainer();
    return Add4(c.items[0]) + c.metadata["max"];
}
