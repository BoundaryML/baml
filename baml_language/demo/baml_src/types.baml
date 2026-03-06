// ── Enums ───────────────────────────────────────────────────────

enum Color {
    Red,
    Green,
    Blue,
}

enum Status {
    Active,
    Inactive,
    Pending,
}

// ── Classes ─────────────────────────────────────────────────────

class Point {
    x int
    y int
}

class Person {
    name string
    age int
    active bool
}

class Container {
    items int[]
    metadata map<string, int>
}

class Pair {
    first int
    second string

    function Swap(self) -> string {
        return self.second;
    }
}

class Counter {
    value int

    function Increment(self) -> int {
        return self.value + 1;
    }

    function Reset(self) -> int {
        return 0;
    }
}

class Builder {
    count int
    label string

    function GetCount(self) -> int {
        return self.count;
    }

    function GetLabel(self) -> string {
        return self.label;
    }
}

// ── Type Aliases ────────────────────────────────────────────────

type MyInt = int
type StringOrInt = string | int
type IntList = int[]
type Callback = (x: int) -> int
type Nested = (x: int) -> (y: int) -> int

// recursive type alias
type Tree = int | Tree[]
