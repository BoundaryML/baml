// Test stream_* class generation for complex/edge cases.

class TreeNode {
    value string
    children TreeNode[]
}

class Container {
    // Nested list
    matrix int[][]
    // Nested list of classes
    grid TreeNode[][]
    // Map of lists
    groups map<string, TreeNode[]>
}

type Foo = int | string
type Bar = Foo | bool
