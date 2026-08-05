// R5 (S15 residue): free-impl method bodies (impl frame, self = for-target)
//
// Inside `implements<T extends I> J for T[]`, the body's frame is the
// impl's generics (rigid `T`, bounded by the declared `extends`), and
// `self` is the for-target `T[]` - so `self[0]` indexes to `T` and
// bound members dispatch on it.
interface PNamed {
    function pname(self) -> string throws never
}

interface PFirst {
    function pfirst(self) -> string throws never
}

implements<T extends PNamed> PFirst for T[] {
    function pfirst(self) -> string {
        let x = self[0];
         // ^ T
        self[0].pname()
    }
}
