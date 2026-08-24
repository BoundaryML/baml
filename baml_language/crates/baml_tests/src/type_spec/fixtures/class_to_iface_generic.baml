// Rigid class values upcast to their generic interface
//
// `fn iter(self) -> Iter<T, never> { self }` inside `class Arr<T>`:
// `self: Arr<T>` (rigid T) flows into the interface-typed return; the
// implements evidence instantiates at the rigid args, so the upcast
// holds like any impl-backed subtype check.
interface CgIter<T, E> {
    function next(self) -> T | null throws E
}

class CgArr<T> {
    xs T[]

    implements CgIter<T, never> {
        function next(self) -> T | null throws never {
            null
        }
    }

    function iter(self) -> CgIter<T, never> throws never {
        self
    }
}

function use_it(a: CgArr<int>) -> CgIter<int, never> throws never {
    let it = a.iter();
     // ^^ user.CgIter<int, never>
    it
}
