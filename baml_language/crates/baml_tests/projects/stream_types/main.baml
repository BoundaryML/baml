// Test default stream type expansion for all type categories.
// No explicit annotations — verifies default D/S computation and stream_* generation.

// ── Primitives ──────────────────────────────────────────────

class Primitives {
    name string
    count int
    flag bool
    score float
}

// ── Enums ───────────────────────────────────────────────────

enum Status {
    Active
    Inactive
}

class WithEnum {
    status Status
}

// ── Literals ────────────────────────────────────────────────

class WithLiterals {
    type "resume"
    version 1
    enabled true
}

// ── Classes ─────────────────────────────────────────────────

class Inner {
    value string
}

class Outer {
    inner Inner
}

// ── Collections ─────────────────────────────────────────────

class WithLists {
    strings string[]
    ints int[]
    classes Inner[]
}

class WithMaps {
    simple map<string, int>
    complex map<string, Inner>
}

class WithNested {
    matrix int[][]
    class_matrix Inner[][]
    nested_map map<string, int[]>
    class_nested_map map<string, Inner[]>
}

// ── Unions & Optionals ──────────────────────────────────────

class WithUnions {
    simple int | string
    with_class Inner | Primitives
    mixed int | Inner
}

class WithOptionals {
    name string?
    inner Inner?
    items int[]?
}

// ── Type Aliases ────────────────────────────────────────────

type SimpleAlias = string

type ClassAlias = Inner

type UnionAlias = int | Inner

type OptionalAlias = Inner?

// ── Dynamic ─────────────────────────────────────────────────

class DynamicClass {
    name string
    @@dynamic
}

// ── All Defaults (comprehensive) ────────────────────────────

class Education {
    school string
    year int
}

type AliasedType = string | int

class AllDefaults {
    name string
    age int
    score float
    active bool
    type "resume"
    version 42
    status Status
    edu Education
    scores int[]
    education Education[]
    nested_edu Education[][]
    edu_map map<string, Education>
    data AliasedType
    mixed int | Education
    maybe string?
    maybe_list int[]?
}

// ── Basic Generation ────────────────────────────────────────

class Resume {
    name string
    age int
    type "resume"
    status Status
    education Education[]
    scores int[]
    metadata map<string, string>
}

type MixedType = int | Education

// ── Complex / Recursive ─────────────────────────────────────

class TreeNode {
    value string
    children TreeNode[]
}

class Container {
    matrix int[][]
    grid TreeNode[][]
    groups map<string, TreeNode[]>
}

type Foo = int | string
type Bar = Foo | bool
