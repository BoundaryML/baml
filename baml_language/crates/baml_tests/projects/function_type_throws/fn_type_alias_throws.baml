// === Function type aliases with throws ===

// Type alias with explicit throws
type ThrowingCallback = () -> int throws string

// Type alias without throws - defaults to throws never
type PureCallback = () -> int

// Type alias with throws never explicitly
type ExplicitPure = () -> int throws never

// Parameterized function type with throws
type Mapper = (int) -> string throws string

// Nested function type - outer throws, inner pure
type Wrapper = (() -> int) -> int throws string
