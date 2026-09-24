// These two definitions intentionally live in the root namespace. The
// to_baml round-trip tests exercise generated source that is recompiled as an
// independent root package; root-scoped dependencies preserve that exact seam.
interface Named {
    type Value;
    name: Self.Value,
}

class StaticChild {
    value: string,
}
