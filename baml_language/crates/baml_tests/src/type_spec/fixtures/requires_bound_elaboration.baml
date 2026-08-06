// requires clauses elaborate into the param env
interface RbeHasKey {
    type Key
    key: Self.Key
}
interface RbeNamed {
    name: string
}
interface RbeEntity requires RbeHasKey<Key = string>, RbeNamed {
}
class RbeUser {
    id: string
    name: string

    implements RbeHasKey {
        type Key = string
        key as id
    }

    implements RbeNamed {
    }

    implements RbeEntity {
    }
}
class RbeBox<T extends RbeEntity> {
    value: T
}
interface RbeSummarizes {
    type Key
    function key(self) -> Self.Key throws never
    function summary(self) -> string throws never
}
implements<T extends RbeEntity> RbeSummarizes for RbeBox<T> {
    type Key = (T as RbeHasKey).Key

    function key(self) -> Self.Key {
        return self.value.as<RbeHasKey<Key = string>>.key
    }

    function summary(self) -> string {
        let tag = self.value.name + ":" + self.key()
         // ^^^ string
        tag
    }
}
function rbe_main() -> string throws never {
    let boxed = RbeBox<RbeUser> { value: RbeUser { id: "u1", name: "Ada" } }
    let s = boxed.summary()
     // ^ string
    s
}
