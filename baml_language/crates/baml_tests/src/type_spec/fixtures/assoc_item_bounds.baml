// members of a rigid projection resolve through the associated type's declared bound
interface AibLabeled {
    function label(self) -> string throws never

    function same_label(self, other: Self) -> bool throws never {
        return self.label() == other.label()
    }
}
interface AibHolder {
    type Item extends AibLabeled

    function get(self) -> Self.Item throws never
}
class AibWidget {
    name: string

    implements AibLabeled {
        function label(self) -> string {
            return self.name
        }
    }
}
class AibWidgetHolder {
    item: AibWidget

    implements AibHolder {
        type Item = AibWidget

        function get(self) -> AibWidget {
            return self.item
        }
    }
}
function aib_label<H extends AibHolder>(holder: H) -> string throws never {
    let item = holder.get()
    let text = item.label()
     // ^^^^ string
    text
}
function aib_same<H extends AibHolder>(holder: H) -> bool throws never {
    let item = holder.get()
    let same = item.same_label(item)
     // ^^^^ bool
    same
}
function aib_main() -> string throws never {
    let holder = AibWidgetHolder { item: AibWidget { name: "alpha" } }
    let text = aib_label(holder)
     // ^^^^ string
    if aib_same(holder) { text } else { "no" }
}
