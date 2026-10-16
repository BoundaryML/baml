// Native backend fixture: object graphs with cycles, the shapes a memory
// manager has to free. Each function builds its graph, reads it back into a
// value both backends agree on, and drops it on return; `native_rust.rs`
// counts what the native heap still holds afterwards (`cycle_leaks`).

class Node { value: int, next: Node | null }

// A node that points at itself.
function self_loop(n: int) -> int {
    let a = Node { value: n, next: null };
    a.next = a;
    let next = a.next;
    if (next == null) { return -1; }
    let node: Node = next;
    node.value + a.value
}

class DNode { value: int, prev: DNode | null, next: DNode | null }

// A doubly linked list of `n` nodes: every neighbour pair is a cycle.
function doubly_linked(n: int) -> int {
    let head = DNode { value: 0, prev: null, next: null };
    let tail = head;
    let i = 1;
    while (i < n) {
        let node = DNode { value: i, prev: tail, next: null };
        tail.next = node;
        tail = node;
        i += 1;
    }
    // Forward, then back again.
    let sum = 0;
    let cursor: DNode | null = head;
    while (cursor != null) {
        let node: DNode = cursor;
        sum += node.value;
        cursor = node.next;
    }
    let back: DNode | null = tail;
    while (back != null) {
        let node: DNode = back;
        sum += node.value * 2;
        back = node.prev;
    }
    sum
}

class TNode { value: int, parent: TNode | null, children: TNode[] }

// A tree `depth` levels deep with two children per node, each child
// pointing back at its parent.
function tree_with_parents(depth: int) -> int {
    let root = TNode { value: 1, parent: null, children: [] };
    grow(root, depth);
    sum_to_root(root)
}

function grow(node: TNode, depth: int) -> void {
    if (depth == 0) { return; }
    let i = 0;
    while (i < 2) {
        let child = TNode { value: node.value * 2 + i, parent: node, children: [] };
        node.children.push(child);
        grow(child, depth - 1);
        i += 1;
    }
}

// Every node's distance to the root, through the parent pointers.
function sum_to_root(node: TNode) -> int {
    let hops = 0;
    let up = node.parent;
    while (up != null) {
        let parent: TNode = up;
        hops += 1;
        up = parent.parent;
    }
    let total = hops + node.value;
    for (let child in node.children) { total += sum_to_root(child); }
    total
}

class GNode { id: int, edges: GNode[] }

// A graph as an array of nodes, each with edges to two others: a ring and
// a chord, so every node is on a cycle.
function graph(n: int) -> int {
    let nodes: GNode[] = [];
    let i = 0;
    while (i < n) {
        nodes.push(GNode { id: i, edges: [] });
        i += 1;
    }
    for (let node in nodes) {
        node.edges.push(nodes[(node.id + 1) % n]);
        node.edges.push(nodes[(node.id * 7 + 3) % n]);
    }
    let sum = 0;
    for (let node in nodes) {
        for (let edge in node.edges) { sum += edge.id * node.id; }
    }
    sum
}

// Request-scoped cycles that each also point at a node that outlives
// them: freeing a cycle must leave the outside node's count as it was.
function cycles_into_live(n: int) -> int {
    let keep = GNode { id: 7, edges: [] };
    let total = 0;
    let i = 0;
    while (i < n) {
        let a = GNode { id: i, edges: [] };
        let b = GNode { id: i + 1, edges: [a, keep] };
        a.edges.push(b);
        total += a.id + b.edges.length();
        i += 1;
    }
    total + keep.id + keep.edges.length()
}

// A closure that captures the array holding it: the array, the closure's
// environment and the captured cell form a cycle.
function closure_in_owner(n: int) -> int {
    let handlers: (() -> int throws never)[] = [];
    handlers.push(() -> int { handlers.length() * 10 });
    handlers.push(() -> int { n + handlers.length() });
    let total = 0;
    for (let handler in handlers) { total += handler(); }
    total
}

class Owner { count: int, items: int[] }

// A closure capturing its owner object, stored where the owner reaches it.
function closure_captures_owner(n: int) -> int {
    let owner = Owner { count: n, items: [] };
    let bump: (() -> int throws never)[] = [];
    bump.push(() -> int {
        owner.count += 1;
        owner.items.push(bump.length());
        owner.count
    });
    bump[0]() + bump[0]() + owner.items.length()
}

class MNode { name: string, links: map<string, MNode> }

// Two nodes linking to each other through maps.
function map_cycle(n: int) -> int {
    let a = MNode { name: "a", links: {} };
    let b = MNode { name: "b", links: {} };
    a.links["peer"] = b;
    b.links["peer"] = a;
    a.links["self"] = a;
    let total = n;
    for (let key in a.links.keys()) { total += a.links[key].name.length(); }
    total + b.links["peer"].links["peer"].name.length()
}

class Link<T> { value: T, next: Link<T> | null }

// A ring of three links through a generic class.
function generic_cycle(n: int) -> int {
    let a = Link<int> { value: n, next: null };
    let b = Link<int> { value: n + 1, next: a };
    let c = Link<int> { value: n + 2, next: b };
    a.next = c;
    let total = 0;
    let cursor: Link<int> | null = a;
    let steps = 0;
    while (steps < 7) {
        if (cursor == null) { return -1; }
        let link: Link<int> = cursor;
        total += link.value;
        cursor = link.next;
        steps += 1;
    }
    total
}

class Leaf { value: int }
class Branch { label: string, child: Branch | Leaf }

// A branch whose child, through a union field, is the branch itself.
function union_cycle(n: int) -> int {
    let b = Branch { label: "b", child: Leaf { value: n } };
    let first = match (b.child) {
        let leaf: Leaf => leaf.value,
        let branch: Branch => -1,
    };
    b.child = b;
    let second = match (b.child) {
        let leaf: Leaf => leaf.value,
        let branch: Branch => branch.label.length(),
    };
    first * 10 + second
}

// A two-node cycle built and then broken: nothing outlives the call.
function built_then_broken(n: int) -> int {
    let total = 0;
    let i = 0;
    while (i < n) {
        let a = Node { value: i, next: null };
        let b = Node { value: i + 1, next: a };
        a.next = b;
        total += a.value + b.value;
        a.next = null;
        i += 1;
    }
    total
}

class Failure { message: string, culprit: Node }

function throw_cycle(n: int) -> int {
    let a = Node { value: n, next: null };
    let b = Node { value: n * 2, next: a };
    a.next = b;
    throw Failure { message: "cycle", culprit: a }
}

// A cycle that leaves its function only inside the thrown error.
function cycle_in_error(n: int) -> int {
    throw_cycle(n) catch (e) {
        let f: Failure => {
            let next = f.culprit.next;
            if (next == null) { -1 } else { let node: Node = next; node.value + f.message.length() }
        },
    }
}

// A cycle the deferred block still reads after the body is done with it.
function cycle_across_defer(n: int) -> string {
    let log: string[] = [];
    {
        let a = Node { value: n, next: null };
        let b = Node { value: n + 1, next: a };
        a.next = b;
        defer {
            let next = a.next;
            if (next != null) { let node: Node = next; log.push("deferred " + node.value.to_string()); }
        }
        log.push("body " + a.value.to_string());
    }
    log.to_string()
}

// A singly linked list of `n` nodes, built and dropped. No cycle: under
// reference counting the drop of the head frees the list recursively.
function long_list(n: int) -> int {
    let head: Node | null = null;
    let i = 0;
    while (i < n) {
        head = Node { value: i, next: head };
        i += 1;
    }
    if (head == null) { return 0; }
    let first: Node = head;
    first.value
}
