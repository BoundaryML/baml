class Gear {
    teeth int
}

// From `gadgets`: `Gear` is bare, the root namespace's `Widget` is `root.Widget`.
function from_nested_namespace() -> Gear {
    root.Widget { id: 1 }
}
