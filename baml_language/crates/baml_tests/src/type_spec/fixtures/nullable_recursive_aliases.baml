// B-1885: unfold union edges with the existing alias resolver, while
// recursive references inside containers retain their own nullability.
type Tree = Tree[] | null
function recursive_list(value: Tree) -> Tree[] {
    let children = value ?? [];
     // ^^^^^^^^ user.Tree[]
    children
}
function recursive_list_guard(value: Tree) -> Tree[] {
    if (value == null) { [] } else {
        let children = value;
         // ^^^^^^^^ user.Tree[]
        children
    }
}
function recursive_list_element(value: Tree) -> void {
    let children = value ?? [];
    let checked: Tree[][] = children;
                         // ^^^^^^^^ mismatch: expected user.Tree[][], got user.Tree[]
}

type MapTree = map<string, MapTree> | null
type AgainMapTree = MapTree | null
function recursive_map(value: AgainMapTree) -> map<string, MapTree> {
    let children = value ?? {};
     // ^^^^^^^^ map<string, user.MapTree>
    children
}
function recursive_map_chain(value: AgainMapTree) -> MapTree | null {
    value?.get("k")
}
function recursive_map_element(value: AgainMapTree) -> void {
    let children = value ?? {};
    let checked: map<string, map<string, MapTree>> = children;
                                                  // ^^^^^^^^ mismatch: expected map<string, map<string, user.MapTree>>, got map<string, user.MapTree>
}

type MutualTree = Branch | null
type Branch = MutualTree[]
type AgainMutualTree = MutualTree | null
function mutual_recursion(value: AgainMutualTree) -> Branch {
    let children = value ?? [];
     // ^^^^^^^^ user.Branch
    children
}
function mutual_chain(value: AgainMutualTree) -> MutualTree | null {
    value?.at(0)
}

// Revisiting a union contributes no new alternatives. Retaining the alias
// at this back-edge would leave null hidden after peeling the outer union.
type RecursiveUnion = RecursiveUnion | RecursiveUnion[] | null
function repeated_union(value: RecursiveUnion) -> RecursiveUnion[] {
    let children = value ?? [];
     // ^^^^^^^^ user.RecursiveUnion[]
    children
}
