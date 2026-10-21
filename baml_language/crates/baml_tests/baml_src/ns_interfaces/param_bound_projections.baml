// An interface parameter's bound may project an associated type through
// another parameter of the same interface, or through `Self`: the bound is
// lowered in the interface's scope, where the projection's qualifying
// interface is one of the bounds that scope declares.

interface PbHasItem {
    type Item
    function item(self) -> Self.Item throws never
}

interface PbTakes<X> {
    function take(self, x: X) -> int throws never
}

// Through an earlier parameter, a later one, `Self`, and a pinned binding.
interface PbEarlier<T extends PbHasItem, U extends PbTakes<T.Item>> {
    function earlier(self, t: T, u: U) -> int throws never
}

interface PbLater<U extends PbTakes<T.Item>, T extends PbHasItem> {
    function later(self, t: T, u: U) -> int throws never
}

interface PbThroughSelf<U extends PbTakes<Self.Out>> {
    type Out
    function out(self) -> Self.Out throws never
    function through_self(self, u: U) -> int throws never
}

interface PbPinned<T extends PbHasItem, U extends PbHasItem<Item = T.Item>> {
    function pinned(self, t: T, u: U) -> T.Item throws never
}

// A required method's own parameter projects too.
interface PbOwnGeneric {
    function own<V extends PbHasItem>(self, v: V) -> V.Item throws never
}

class PbBag {
    n: int,

    implements PbHasItem {
        type Item = int
        function item(self) -> int throws never { self.n }
    }
}

class PbAddOne {
    implements PbTakes<int> {
        function take(self, x: int) -> int throws never { x + 1 }
    }
}

class PbImpl {
    implements PbEarlier<PbBag, PbAddOne> {
        function earlier(self, t: PbBag, u: PbAddOne) -> int throws never { u.take(t.item()) }
    }
    implements PbLater<PbAddOne, PbBag> {
        function later(self, t: PbBag, u: PbAddOne) -> int throws never { u.take(t.item()) }
    }
    implements PbThroughSelf<PbAddOne> {
        type Out = int
        function out(self) -> int throws never { 10 }
        function through_self(self, u: PbAddOne) -> int throws never { u.take(self.out()) }
    }
    implements PbPinned<PbBag, PbBag> {
        function pinned(self, t: PbBag, u: PbBag) -> int throws never { t.item() + u.item() }
    }
    implements PbOwnGeneric {
        function own<V extends PbHasItem>(self, v: V) -> V.Item throws never { v.item() }
    }
}

function pb_earlier<I extends PbEarlier<PbBag, PbAddOne>>(i: I) -> int throws never {
    i.earlier(PbBag { n: 4 }, PbAddOne {})
}

test "an interface parameter bound projects through its sibling parameters" {
    let i = PbImpl {};
    assert.equal(pb_earlier(i), 5);
    assert.equal(i.later(PbBag { n: 6 }, PbAddOne {}), 7);
    assert.equal(i.through_self(PbAddOne {}), 11);
    assert.equal(i.pinned(PbBag { n: 1 }, PbBag { n: 2 }), 3);
    assert.equal(i.own(PbBag { n: 9 }), 9)
}
