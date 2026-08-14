// Intersection bounds (`T extends A & B`) executed end-to-end.
//
// A bound list is a conjunction: an argument must satisfy every conjunct, and
// `T`'s member surface is the union of all of them. CST lowering used to keep
// only the first conjunct, so every member reached through a later one was
// invisible and every later bound went unenforced.
//
// Rejection cases live in `projects/diagnostic_errors/generic_intersection_bounds`;
// the accepted declaration shapes are snapshotted through TIR/MIR in
// `projects/compiles/generic_intersection_bounds`.

interface IbNamed { name: string }

interface IbSized {
    function size(self) -> int throws never
}

interface IbTagged {
    type Label
    function tag(self) -> Self.Label throws never
}

class IbBoth {
    name: string

    implements IbNamed {}

    implements IbSized {
        function size(self) -> int { return 7 }
    }

    implements IbTagged {
        type Label = string
        function tag(self) -> string { return "t:" + self.name }
    }
}

// Both conjuncts contribute members: `name` from `IbNamed`, `size()` from
// `IbSized`.
function ib_describe<T extends IbNamed & IbSized>(item: T) -> string {
    return item.name + ":" + item.size().to_string()
}

test "intersection_bound_reaches_members_of_every_conjunct" {
    assert.equal(ib_describe<IbBoth>(IbBoth { name: "b" }), "b:7")
}

// A member declared by the *last* of three conjuncts is still in scope.
function ib_label<T extends IbNamed & IbSized & IbTagged<Label = string>>(item: T) -> string {
    return item.tag() + "/" + item.size().to_string()
}

test "intersection_bound_reaches_member_of_trailing_conjunct" {
    assert.equal(ib_label<IbBoth>(IbBoth { name: "b" }), "t:b/7")
}

// Conjunct order is not significant — the same conjunction either way.
function ib_describe_reordered<T extends IbSized & IbNamed>(item: T) -> string {
    return item.name + ":" + item.size().to_string()
}

test "intersection_bound_is_order_insensitive" {
    assert.equal(
        ib_describe_reordered<IbBoth>(IbBoth { name: "b" }),
        ib_describe<IbBoth>(IbBoth { name: "b" }),
    )
}

// A blanket `implements` over an intersection dispatches through the concrete
// implementor at runtime.
interface IbPrintable {
    function display(self) -> string throws never
}

implements<T extends IbNamed & IbSized> IbPrintable for T {
    function display(self) -> string {
        return ib_describe<T>(self)
    }
}

class IbOther {
    name: string

    implements IbNamed {}

    implements IbSized {
        function size(self) -> int { return 2 }
    }
}

test "blanket_impl_over_intersection_dispatches_per_implementor" {
    let a: IbPrintable = IbBoth { name: "x" }
    let b: IbPrintable = IbOther { name: "y" }
    assert.equal(a.display(), "x:7")
    assert.equal(b.display(), "y:2")
}

// A type constructor carries the conjunction through to its field's type.
class IbHolder<T extends IbNamed & IbSized> {
    item: T
}

function ib_holder_summary(holder: IbHolder<IbBoth>) -> string {
    return holder.item.name + "=" + holder.item.size().to_string()
}

test "intersection_bound_on_type_constructor_keeps_member_surface" {
    assert.equal(
        ib_holder_summary(IbHolder<IbBoth> { item: IbBoth { name: "h" } }),
        "h=7",
    )
}

// An interface method signature's own generics carry a conjunction, and an
// override may spell the conjuncts in a different order.
interface IbRegistry {
    function summarize<U extends IbNamed & IbSized>(self, item: U) -> string throws never
}

class IbRegistered {
    implements IbRegistry {
        function summarize<U extends IbSized & IbNamed>(self, item: U) -> string {
            return ib_describe<U>(item)
        }
    }
}

test "interface_method_generic_intersection_bound_runs" {
    let r = IbRegistered {}
    assert.equal(r.summarize<IbBoth>(IbBoth { name: "r" }), "r:7")
}
