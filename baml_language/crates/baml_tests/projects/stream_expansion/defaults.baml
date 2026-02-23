// Stream expansion default rules - no user attributes.
// Uses Education and Status from base_types.baml.

// Tests: primitives, class ref, enum ref, containers, unions, literals, optional
class DefaultExpansion {
    // Primitives: → null | T
    name string
    count int
    score float
    flag bool

    // Enum ref: → null | Status (no stream. prefix on enums)
    status Status

    // Class ref: → null | stream.Education
    edu Education

    // List of primitives: → string[] (no null, starts empty)
    tags string[]

    // List of classes: → stream.Education[] (no null)
    education Education[]

    // Map: → map<string, int> (no null, starts empty)
    metadata map<string, int>

    // Map with class value: → map<string, stream.Education> (no null)
    nested_map map<string, Education>

    // Union: → null | int | string
    value int | string

    // Union with class: → null | stream.Education | int
    mixed Education | int

    // Optional: → string | null
    opt string?

    // Optional class: → stream.Education | null
    opt_edu Education?

    // String literal: → "resume" (S=never, literal passthrough)
    tag "resume"

    // Int literal: → 42 (S=never)
    code 42

    // Bool literal: → true (S=never)
    yes true
}

// Nested containers
class NestedContainers {
    // Nested list: → stream.Education[][] (no null)
    matrix Education[][]

    // Map of lists: → map<string, stream.Education[]>
    map_of_lists map<string, Education[]>

    // List of maps: → map<string, stream.Education>[]
    list_of_maps map<string, Education>[]
}

// Recursive type
class TreeNode {
    label string
    children TreeNode[]
}

// Dynamic class should still get stream.* variant
class DynamicClass {
    name string
    @@dynamic
}

// Type aliases should get stream.* variants
type AliasUnion = Education | string

type AliasArray = Education[]

type AliasPrimitive = int
