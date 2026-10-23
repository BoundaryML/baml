// An interface pattern with no written arguments takes them from the
// scrutinee: an existential of the interface, or the one instantiation at
// which the scrutinee's implementors implement it.

interface IpaSrc<T> {
    value: T
}

class IpaInt {
    value: int
    implements IpaSrc<int> {}
}

class IpaNamed {
    count: int
    implements IpaSrc<int> {
        value as count
    }
}

function ipa_class(x: IpaInt) -> int {
    match (x) {
        IpaSrc { value } => value,
    }
}

test "an interface pattern adopts the arguments a class implements it at" {
    assert.equal(ipa_class(IpaInt { value: 7 }), 7)
}

function ipa_union(x: IpaNamed | string) -> int {
    match (x) {
        IpaSrc { value } => value,
        _ => 0,
    }
}

test "an interface pattern over a union reads a linked field" {
    assert.equal(ipa_union(IpaNamed { count: 4 }), 4);
    assert.equal(ipa_union("s"), 0)
}

function ipa_bound<T extends IpaSrc<int>>(x: T) -> int {
    match (x) {
        IpaSrc { value } => value,
        _ => 0,
    }
}

test "an interface pattern adopts a type parameter's bound" {
    assert.equal(ipa_bound(IpaInt { value: 9 }), 9)
}

interface IpaBox {
    type Item
    value: Self.Item
}

class IpaIntBox {
    value: int
    implements IpaBox {
        type Item = int
    }
}

class IpaOtherIntBox {
    value: int
    implements IpaBox {
        type Item = int
    }
}

class IpaStringBox {
    value: string
    implements IpaBox {
        type Item = string
    }
}

function ipa_pinned(x: IpaIntBox | IpaStringBox) -> int {
    match (x) {
        IpaBox<Item = int> { value } => value + 1,
        _ => 0,
    }
}

test "an interface pattern's written pin types the field" {
    assert.equal(ipa_pinned(IpaIntBox { value: 1 }), 2);
    assert.equal(ipa_pinned(IpaStringBox { value: "s" }), 0)
}

function ipa_adopted_pin(x: IpaIntBox | IpaOtherIntBox) -> int {
    match (x) {
        IpaBox { value } => value + 1,
        _ => 0,
    }
}

test "an interface pattern adopts the one pin its implementors share" {
    assert.equal(ipa_adopted_pin(IpaOtherIntBox { value: 1 }), 2)
}

function ipa_existential_pin(x: IpaBox<Item = int>) -> int {
    match (x) {
        IpaBox { value } => value + 1,
    }
}

test "an interface pattern adopts an existential's pin" {
    assert.equal(ipa_existential_pin(IpaIntBox { value: 1 }), 2)
}

function ipa_pinned_in_list(x: IpaIntBox[] | string[] | IpaStringBox[]) -> int {
    match (x) {
        [IpaBox<Item = int> { value }] => value,
        _ => 0,
    }
}

test "an interface sub-pattern claims the lists of its implementors" {
    assert.equal(ipa_pinned_in_list([IpaIntBox { value: 3 }]), 3);
    assert.equal(ipa_pinned_in_list([IpaStringBox { value: "s" }]), 0);
    assert.equal(ipa_pinned_in_list(["s"]), 0)
}

function ipa_thrower(fail: bool) -> string throws IpaInt {
    if (fail) {
        throw IpaInt { value: 1 };
    }
    "x"
}

function ipa_catch_arm(fail: bool) -> string {
    ipa_thrower(fail) catch (e) {
        IpaSrc { value } => `src ${value}`,
    }
}

test "an interface pattern in a catch arm adopts from the errors that reach it" {
    assert.equal(ipa_catch_arm(true), "src 1");
    assert.equal(ipa_catch_arm(false), "x")
}
