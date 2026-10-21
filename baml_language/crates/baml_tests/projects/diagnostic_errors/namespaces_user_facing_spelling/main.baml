// A diagnostic spells a type the way the file it is reported in writes it:
// a declaration of the file's own namespace bare, any other declaration of
// the package `root.`-qualified (TYPE_SYSTEM.md § Declarations, Packages, and
// Names). Every mismatch below names one of each.

class Widget {
    id int
}

// From the root namespace: `Widget` is bare, `gadgets.Gear` is `root.gadgets.Gear`.
function from_root_namespace() -> Widget {
    root.gadgets.Gear { teeth: 7 }
}
