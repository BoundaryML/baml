// B-1885: get()/at() add null to an already nullable element. The lookup
// must be one flat nullable union before ?. resolves a member. Check the
// inferred result itself, including null, rather than only a coalesced value.
class Group { text: string }
type MaybeGroup = Group | null
type Again = MaybeGroup | null

function map_lookup(m: map<string, Group?>) -> string {
    let item = m.get("k");
     // ^^^^ user.Group | null
    let text = m.get("k")?.text;
     // ^^^^ string | null
    text ?? ""
}

function array_lookup(xs: (Group | null)[]) -> string {
    let item = xs.at(1);
     // ^^^^ user.Group | null
    let text = xs.at(1)?.text;
     // ^^^^ string | null
    text ?? ""
}

function regex_lookup(text: string) -> string {
    let m = baml.regex.new("(a)").match_once(text);
    if let found: baml.regex.Match = m {
        let group = found.groups.at(1);
         // ^^^^^ baml.regex.Group | null
        let value = found.groups.at(1)?.text;
         // ^^^^^ string | null
        value ?? ""
    } else { "" }
}

function alias_lookup(m: map<string, Again>, xs: Again[]) -> string {
    let a = m.get("k")?.text;
     // ^ string | null
    let b = xs.at(1)?.text;
     // ^ string | null
    (a ?? "") + (b ?? "")
}

function nullable<T>(value: T) -> T? { value }
function generic_lookup(value: Again) -> string | null {
    let text = nullable<Again>(value)?.text;
     // ^^^^ string | null
    text
}

// Removing receiver null must leave a field's own nullability intact.
class Nested { child: Group? }
function nested_field(xs: Nested?[]) -> Group | null {
    let child = xs.at(0)?.child;
     // ^^^^^ user.Group | null
    child
}

// Removing outer null must also preserve null inside unrelated containers.
function nested_list(m: map<string, (Group | null)[]?>) -> (Group | null)[] {
    let items = m.get("k") ?? [];
     // ^^^^^ (user.Group | null)[]
    items
}

function nested_map(m: map<string, map<string, Group?>?>) -> map<string, Group?> {
    let items = m.get("k") ?? {};
     // ^^^^^ map<string, user.Group | null>
    items
}

// The ticket's workarounds must agree with direct optional access.
function coalesce_guard(m: map<string, Group?>) -> string {
    let g = m.get("k") ?? null;
    if (g == null) { "" } else {
        let text = g.text;
         // ^^^^ string
        text
    }
}
function index_lookup(xs: (Group | null)[]) -> string {
    let text = xs[1]?.text;
     // ^^^^ string | null
    text ?? ""
}
function match_lookup(xs: (Group | null)[]) -> string {
    match (xs.at(1)) {
        let group: Group => group.text,
        _ => "",
    }
}

function alias_direct(value: Again) -> string | null {
    let text = value?.text;
     // ^^^^ string | null
    text
}
function alias_guard(value: Again) -> string {
    if (value == null) { "" } else {
        let text = value.text;
         // ^^^^ string
        text
    }
}
function alias_coalesce(value: Again) -> Group {
    let item = value ?? Group { text: "" };
     // ^^^^ user.Group
    item
}

// These must remain errors: removing receiver null cannot narrow the payload.
function retain_list_null(m: map<string, (Group | null)[]?>) -> void {
    let items = m.get("k") ?? [];
    let checked: Group[] = items;
                        // ^^^^^ mismatch: expected user.Group[], got (user.Group | null)[]
}
function retain_map_null(m: map<string, map<string, Group?>?>) -> void {
    let items = m.get("k") ?? {};
    let checked: map<string, Group> = items;
                                   // ^^^^^ mismatch: expected map<string, user.Group>, got map<string, user.Group | null>
}
function retain_field_null(xs: Nested?[]) -> void {
    let child = xs.at(0)?.child;
    let checked: Group = child;
                      // ^^^^^ mismatch: expected user.Group, got user.Group | null
}

type NamedCallback = ((first: string, second: string) -> string throws never)?
type AgainCallback = NamedCallback | null
function named_callback(callback: AgainCallback) -> string | null {
    let value = callback?.(second = "b", first = "a");
     // ^^^^^ string | null
    value
}

// Union flattening must retain every non-null alternative.
interface HasText { text: string }
class Left { text: string, implements HasText {} }
class Right { text: string, implements HasText {} }
type Choice = Left | Right | null
type AgainChoice = Choice | null
function either_choice(value: AgainChoice) -> string | null {
    let text = value?.text;
     // ^^^^ string | null
    text
}

// Alias expansion is not null removal. Optional indexing of this non-null
// receiver must not acquire a null arm merely because Ints unfolds.
type Ints = int[]
function nonnullable_alias_index(xs: Ints | string[]) -> int | string {
    let value = xs?.[0];
     // ^^^^^ int | string
    value
}

// Null removal changes the union, not the spelling of surviving leaf aliases.
type Count = int
type MaybeCount = Count | null
function preserve_primitive_alias(value: MaybeCount) -> Count {
    let count = value ?? 0;
     // ^^^^^ user.Count
    count
}
type NamedGroup = Group
type MaybeNamedGroup = NamedGroup | null
function preserve_class_alias(value: MaybeNamedGroup) -> NamedGroup {
    let group = value ?? Group { text: "" };
     // ^^^^^ user.NamedGroup
    group
}
type Nothing = null
function aliased_null(value: Group | Nothing) -> string | null {
    let text = value?.text;
     // ^^^^ string | null
    text
}
function duplicate_class_alias(value: NamedGroup | Group | null) -> string | null {
    let text = value?.text;
     // ^^^^ string | null
    text
}
