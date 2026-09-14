// `default.` reads interface contract FIELDS
//
// `default.x` resolves any member the INTERFACE side gives meaning to:
// declared fields (contract state, read through the interface view)
// and default-bodied methods (delegation). Only bodyless required
// methods are excluded - there is nothing to delegate to.
interface DfNamed {
    name: string

    function describe(self) -> string throws never {
        "x"
    }
}

class DfP {
    name: string

    implements DfNamed {
        function describe(self) -> string throws never {
            let n = default.name;
             // ^ string
            n + self.name
        }
    }
}

function df_field_use(p: DfP) -> string throws never {
    p.describe()
}
