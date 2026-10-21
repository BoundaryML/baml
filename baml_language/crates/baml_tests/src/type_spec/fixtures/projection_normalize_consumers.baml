// projections normalize at structure points and operators
//
// rustc's structurally_resolve_type NORMALIZES as well as resolving:
// a ground reducible projection is not structure. `(T as Source).Item`
// coming back from a call IS `string` wherever a consumer demands
// shape - operator dispatch, method receivers, field bases.
interface PnSource {
    type Item

    function get(self) -> Self.Item throws never
}

class PnText {
    text string

    implements PnSource {
        type Item = string

        function get(self) -> string throws never {
            self.text
        }
    }
}

function pn_take<T extends PnSource<Item = string>>(source: T) -> T.Item throws never {
    source.get()
}

function pn_concat(t: PnText) -> string throws never {
    let s = pn_take(t) + ":";
     // ^ string
    s
}

function pn_chain(t: PnText) -> int throws never {
    let n = pn_take(t).length();
     // ^ int
    n
}
