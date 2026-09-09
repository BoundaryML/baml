// Probe field links and aliasing for mutable values loaded through an
// interface existential. The backing fields intentionally use different names
// so that every virtual access must follow the implementation's field links.

class Child {
    value: int,
}

class Snapshot {
    count: int,
    first_item: int,
    item_count: int,
    map_a: int?,
    map_b: int?,
    child_value: int,
}

interface MutableView {
    count: int,
    items: int[],
    lookup: map<string, int>,
    child: Child,

    function snapshot(self) -> Snapshot throws never {
        Snapshot {
            count: self.count,
            first_item: self.items[0],
            item_count: self.items.length(),
            map_a: self.lookup.get("a"),
            map_b: self.lookup.get("b"),
            child_value: self.child.value,
        }
    }
}

class Owner {
    stored_count: int,
    stored_items: int[],
    stored_lookup: map<string, int>,
    stored_child: Child,

    implements MutableView {
        count as stored_count
        items as stored_items
        lookup as stored_lookup
        child as stored_child
    }
}

class FieldMutationObservation {
    concrete_scalar_seen_through_interface: int,
    interface_scalar_seen_through_concrete: int,
    default_after_alias_mutations: Snapshot,
    concrete_list_first_after_alias_mutation: int,
    concrete_list_count_after_alias_mutation: int,
    concrete_map_a_after_alias_mutation: int?,
    concrete_map_b_after_alias_mutation: int?,
    concrete_child_after_alias_mutation: int,
    default_after_root_replacement: Snapshot,
    concrete_list_first_after_old_alias_mutation: int,
    concrete_list_count_after_old_alias_mutation: int,
    concrete_map_a_after_old_alias_mutation: int?,
    concrete_map_b_after_old_alias_mutation: int?,
    concrete_child_after_old_alias_mutation: int,
    detached_list_first: int,
    detached_list_count: int,
    detached_map_a: int?,
    detached_map_b: int?,
    detached_child_value: int,
}

function observe_interface_field_mutability() -> FieldMutationObservation throws never {
    let owner = Owner {
        stored_count: 1,
        stored_items: [10],
        stored_lookup: { "a": 1 },
        stored_child: Child { value: 1 },
    };
    let view: MutableView = owner;

    // Both directions reach the one physical slot selected by the field link.
    owner.stored_count = 2;
    let concrete_scalar_seen_through_interface = view.count;
    view.count = 3;
    let interface_scalar_seen_through_concrete = owner.stored_count;

    // A virtual field load of a container/class returns its identity-bearing
    // value. Mutating that value through a local alias reaches the owner.
    let items_alias = view.items;
    let lookup_alias = view.lookup;
    let child_alias = view.child;
    items_alias[0] = 11;
    items_alias.push(20);
    lookup_alias["a"] = 11;
    lookup_alias["b"] = 2;
    child_alias.value = 12;

    let default_after_alias_mutations = view.snapshot();
    let concrete_list_first_after_alias_mutation = owner.stored_items[0];
    let concrete_list_count_after_alias_mutation = owner.stored_items.length();
    let concrete_map_a_after_alias_mutation = owner.stored_lookup.get("a");
    let concrete_map_b_after_alias_mutation = owner.stored_lookup.get("b");
    let concrete_child_after_alias_mutation = owner.stored_child.value;

    // Replacing a virtual field overwrites the physical root slot. Existing
    // aliases still point to the former list/map/child, now detached from owner.
    view.items = [99];
    view.lookup = { "a": 9 };
    view.child = Child { value: 8 };
    items_alias[0] = 111;
    items_alias.push(30);
    lookup_alias["a"] = 111;
    lookup_alias["b"] = 22;
    child_alias.value = 120;

    FieldMutationObservation {
        concrete_scalar_seen_through_interface: concrete_scalar_seen_through_interface,
        interface_scalar_seen_through_concrete: interface_scalar_seen_through_concrete,
        default_after_alias_mutations: default_after_alias_mutations,
        concrete_list_first_after_alias_mutation: concrete_list_first_after_alias_mutation,
        concrete_list_count_after_alias_mutation: concrete_list_count_after_alias_mutation,
        concrete_map_a_after_alias_mutation: concrete_map_a_after_alias_mutation,
        concrete_map_b_after_alias_mutation: concrete_map_b_after_alias_mutation,
        concrete_child_after_alias_mutation: concrete_child_after_alias_mutation,
        default_after_root_replacement: view.snapshot(),
        concrete_list_first_after_old_alias_mutation: owner.stored_items[0],
        concrete_list_count_after_old_alias_mutation: owner.stored_items.length(),
        concrete_map_a_after_old_alias_mutation: owner.stored_lookup.get("a"),
        concrete_map_b_after_old_alias_mutation: owner.stored_lookup.get("b"),
        concrete_child_after_old_alias_mutation: owner.stored_child.value,
        detached_list_first: items_alias[0],
        detached_list_count: items_alias.length(),
        detached_map_a: lookup_alias.get("a"),
        detached_map_b: lookup_alias.get("b"),
        detached_child_value: child_alias.value,
    }
}

test "interface fields preserve mutable aliases and root replacement detaches them" {
    let observed = observe_interface_field_mutability();
    assert.equal(observed.concrete_scalar_seen_through_interface, 2);
    assert.equal(observed.interface_scalar_seen_through_concrete, 3);
    assert.equal(
        observed.default_after_alias_mutations,
        Snapshot { count: 3, first_item: 11, item_count: 2, map_a: 11, map_b: 2, child_value: 12 },
    );
    assert.equal(observed.concrete_list_first_after_alias_mutation, 11);
    assert.equal(observed.concrete_list_count_after_alias_mutation, 2);
    assert.equal(observed.concrete_map_a_after_alias_mutation, 11);
    assert.equal(observed.concrete_map_b_after_alias_mutation, 2);
    assert.equal(observed.concrete_child_after_alias_mutation, 12);
    assert.equal(
        observed.default_after_root_replacement,
        Snapshot { count: 3, first_item: 99, item_count: 1, map_a: 9, map_b: null, child_value: 8 },
    );
    assert.equal(observed.concrete_list_first_after_old_alias_mutation, 99);
    assert.equal(observed.concrete_list_count_after_old_alias_mutation, 1);
    assert.equal(observed.concrete_map_a_after_old_alias_mutation, 9);
    assert.equal(observed.concrete_map_b_after_old_alias_mutation, null);
    assert.equal(observed.concrete_child_after_old_alias_mutation, 8);
    assert.equal(observed.detached_list_first, 111);
    assert.equal(observed.detached_list_count, 3);
    assert.equal(observed.detached_map_a, 111);
    assert.equal(observed.detached_map_b, 22);
    assert.equal(observed.detached_child_value, 120)
}
