// Test: Function type in type alias with named parameter
type IntToInt = (x: int) -> int throws never

// Test: Function type with multiple named parameters
type BinaryOp = (a: int, b: int) -> int throws never

// Test: Function type without parameter names
type UnnamedParams = (int, int) -> int throws never

// Test: Mixed named and unnamed parameters
type MixedParams = (x: int, int, z: string) -> bool throws never

// Test: No parameters
type Producer = () -> string throws never

// Test: Nested function types (curried)
type Curried = (x: int) -> ((y: int) -> int throws never) throws never

// Test: Complex parameter types
type Complex = (items: int[], lookup: map<string, int>) -> string? throws never

// Test: Function type as return type of a function
function GetTransform() -> (x: int) -> int throws never {
    // For now just return a placeholder - actual function values come later
    1
}

// Test: Function type with union parameter
type UnionParam = (value: int | string) -> bool throws never

// Test: Function type returning a function
type FunctionFactory = (config: string) -> ((int) -> int throws never) throws never
