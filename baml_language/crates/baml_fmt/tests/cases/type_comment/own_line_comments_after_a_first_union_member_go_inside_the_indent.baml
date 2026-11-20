type V = int
    /* own */
    | string
    // own2
    | bool;
type Values = int[/*C2
X*/] /* first */ | /* arm */ string? | /* last */ bool;
function f<T extends A
    /* own */
    & B
    // own2
    & C>(x: T) -> int { 1 }
