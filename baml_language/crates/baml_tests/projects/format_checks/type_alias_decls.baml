// Testing all the formatter cases for type alias declarations
// as well as complex type formatting permutations

// ===== Simple type alias =====
type SimpleAlias = int

// ===== Alias to another alias =====
type AliasChain = SimpleAlias

// ===== Alias to string =====
type Name = string

// ===== String literal type =====
type Role = "user" | "assistant" | "system"

// ===== Integer literal type =====
type HttpOk = 200

// ===== Boolean literal type =====
type AlwaysTrue = true

// ===== Union of literals =====
type SuccessCode = 200 | 201 | 204

// ===== Mixed literal union =====
type MixedLiterals = 200 | "success" | true

// ===== Optional type =====
type MaybeInt = int?

// ===== Array type =====
type IntList = int[]

// ===== Nested array type =====
type Matrix = int[][]

// ===== Generic type =====
type StringMap = map<string, int>

// ===== Complex generic =====
type ComplexMap = map<string, int | string | bool>

// ===== Nested generic =====
type NestedMap = map<string, map<string, int[]>>

// ===== Parenthesized union =====
type ParenUnion = (int | string)[]

// ===== Optional union =====
type OptionalUnion = (int | string)?

// ===== Recursive type =====
type Json = int | float | string | bool | null | Json[] | map<string, Json>

// ===== Function type =====
type IntToInt = (x: int) -> int

// ===== Function type with multiple params =====
type BinaryOp = (a: int, b: int) -> int

// ===== Function type without param names =====
type UnnamedParams = (int, int) -> int

// ===== Function type no params =====
type Producer = () -> string

// ===== Nested function type (curried) =====
type Curried = (x: int) -> (y: int) -> int

// ===== Function type with complex params =====
type ComplexFn = (items: int[], lookup: map<string, int>) -> string?

// ===== Function type with union param =====
type UnionParamFn = (value: int | string) -> bool

// ===== Function type returning function =====
type FnFactory = (config: string) -> (int) -> int

// ===== Long union exceeding limit: many string literals =====
type LongUnion = "alpha" | "bravo" | "charlie" | "delta" | "echo" | "foxtrot" | "golf" | "hotel" | "india" | "juliet" | "kilo" | "lima"

// ===== Long union exceeding limit: many types =====
type WideUnion = int | string | bool | float | null | int[] | string[] | bool[] | map<string, int> | map<string, string>

// ===== Very long name + type exceeding limit =====
type ThisIsAnExtremelyLongTypeAliasNameThatDefinitelyExceedsLimit = int | string | bool | float | null

// ===== Long name + long union exceeding limit =====
type VeryLongNamePlusLongUnion = "alpha" | "bravo" | "charlie" | "delta" | "echo" | "foxtrot" | "golf" | "hotel" | "india"

// ===== Deeply nested generics exceeding limit =====
type DeepGeneric = map<string, map<string, map<string, map<string, int | string | bool | float | null>>>>

// ===== Deeply nested function type exceeding limit =====
type DeepFnType = (x: int) -> (y: int) -> (z: int) -> (w: int) -> map<string, int | string | bool>

// ===== Function type with many long params exceeding limit =====
type LongParamFn = (first_parameter: map<string, int>, second_parameter: (int | string)[], third_parameter: map<string, bool>) -> string

// ===== Nested paren unions =====
type NestedParenUnion = ((int | string) | (bool | float))[]
type DeepParenUnion = (((int | string) | (bool | float)) | (null | int[]))[]?

// ===== Complex recursive type exceeding limit =====
type Tree = string | map<string, Tree> | Tree[] | map<string, map<string, Tree | string | int | null>>

// ===== Postfix operators chained =====
type OptionalArray = int?[]
type ArrayOptional = int[]?
type DoubleOptional = int??
type DoubleArray = int[][]
type ComplexPostfix = (int | string)?[][]
type DeepPostfix = ((int | string | bool | float)?[])[]?

// ===== Constrained types exceeding limit =====
type Constrained = int @check("positive", {{ this > 0 }})
type ConstrainedLong = map<string, int | string | bool> @check("valid", {{ this != null }})

// ===== Union of classes =====
class TypeAliasSuccess {
    data string
}

class TypeAliasFailure {
    reason string
}

type Result = TypeAliasSuccess | TypeAliasFailure

// ===== Optional of union alias =====
type MaybeResult = Result?

// ===== Trivia on type alias (line comments) =====
// 1 type keyword leading
type // 2 type keyword trailing
// 3 name leading
TriviaAlias // 4 name trailing
// 5 equals leading
= // 6 equals trailing
// 7 type leading
int // 8 type trailing

///////////////////

// ===== Trivia on type alias (block comments) =====
/* 1 type keyword leading */
type /* 2 type keyword trailing */
/* 3 name leading */
TriviaAliasBlock /* 4 name trailing */
/* 5 equals leading */
= /* 6 equals trailing */
/* 7 type leading */
int /* 8 type trailing */

///////////////////

// ===== Trivia in union type (line comments) =====
type TriviaUnion =
    // 1 first member leading
    int // 2 first member trailing
    // 3 pipe leading
    | // 4 pipe trailing
    // 5 second member leading
    string // 6 second member trailing
    // 7 second pipe leading
    | // 8 second pipe trailing
    // 9 third member leading
    bool // 10 third member trailing

// ===== Trivia in union type (block comments) =====
type TriviaUnionBlock =
    /* 1 first member leading */
    int /* 2 first member trailing */
    /* 3 pipe leading */
    | /* 4 pipe trailing */
    /* 5 second member leading */
    string /* 6 second member trailing */
    /* 7 second pipe leading */
    | /* 8 second pipe trailing */
    /* 9 third member leading */
    bool /* 10 third member trailing */

// ===== Trivia in long wrapping union (line comments) =====
type TriviaLongUnion =
    // 1 leading
    "alpha" // 2 trailing
    // 3 leading
    | // 4 trailing
    // 5 leading
    "bravo" // 6 trailing
    // 7 leading
    | // 8 trailing
    // 9 leading
    "charlie" // 10 trailing
    // 11 leading
    | // 12 trailing
    // 13 leading
    "delta" // 14 trailing
    // 15 leading
    | // 16 trailing
    // 17 leading
    "echo" // 18 trailing
    // 19 leading
    | // 20 trailing
    // 21 leading
    "foxtrot" // 22 trailing
    // 23 leading
    | // 24 trailing
    // 25 leading
    "golf" // 26 trailing
    // 27 leading
    | // 28 trailing
    // 29 leading
    "hotel" // 30 trailing

// ===== Trivia in long wrapping union (block comments) =====
type TriviaLongUnionBlock =
    /* 1 leading */
    "alpha" /* 2 trailing */
    /* 3 leading */
    | /* 4 trailing */
    /* 5 leading */
    "bravo" /* 6 trailing */
    /* 7 leading */
    | /* 8 trailing */
    /* 9 leading */
    "charlie" /* 10 trailing */
    /* 11 leading */
    | /* 12 trailing */
    /* 13 leading */
    "delta" /* 14 trailing */
    /* 15 leading */
    | /* 16 trailing */
    /* 17 leading */
    "echo" /* 18 trailing */
    /* 19 leading */
    | /* 20 trailing */
    /* 21 leading */
    "foxtrot" /* 22 trailing */
    /* 23 leading */
    | /* 24 trailing */
    /* 25 leading */
    "golf" /* 26 trailing */
    /* 27 leading */
    | /* 28 trailing */
    /* 29 leading */
    "hotel" /* 30 trailing */

// ===== Trivia in generic type (line comments) =====
type TriviaGeneric =
    map // 1 base type trailing
    // 2 open angle leading
    < // 3 open angle trailing
    // 4 first arg leading
    string // 5 first arg trailing
    , // 6 comma trailing
    // 7 second arg leading
    int // 8 second arg trailing
    // 9 close angle leading
    > // 10 close angle trailing

// ===== Trivia in generic type (block comments) =====
type TriviaGenericBlock =
    map /* 1 base type trailing */
    /* 2 open angle leading */
    < /* 3 open angle trailing */
    /* 4 first arg leading */
    string /* 5 first arg trailing */
    , /* 6 comma trailing */
    /* 7 second arg leading */
    int /* 8 second arg trailing */
    /* 9 close angle leading */
    > /* 10 close angle trailing */

// ===== Trivia in deeply nested generic (line comments) =====
type TriviaDeepGeneric =
    map // 1 trailing
    < // 2 trailing
    // 3 leading
    string // 4 trailing
    , // 5 trailing
    // 6 leading
    map // 7 trailing
    < // 8 trailing
    // 9 leading
    string // 10 trailing
    , // 11 trailing
    // 12 leading
    map // 13 trailing
    < // 14 trailing
    // 15 leading
    string // 16 trailing
    , // 17 trailing
    // 18 leading
    int // 19 trailing
    | // 20 trailing
    // 21 leading
    string // 22 trailing
    > // 23 trailing
    > // 24 trailing
    > // 25 trailing

// ===== Trivia in function type (line comments) =====
type TriviaFnType =
    // 1 open paren leading
    ( // 2 open paren trailing
    // 3 param leading
    x // 4 param name trailing
    : // 5 colon trailing
    // 6 type leading
    int // 7 type trailing
    // 8 close paren leading
    ) // 9 close paren trailing
    // 10 arrow leading
    -> // 11 arrow trailing
    // 12 return type leading
    string // 13 return type trailing

// ===== Trivia in function type (block comments) =====
type TriviaFnTypeBlock =
    /* 1 open paren leading */
    ( /* 2 open paren trailing */
    /* 3 param leading */
    x /* 4 param name trailing */
    : /* 5 colon trailing */
    /* 6 type leading */
    int /* 7 type trailing */
    /* 8 close paren leading */
    ) /* 9 close paren trailing */
    /* 10 arrow leading */
    -> /* 11 arrow trailing */
    /* 12 return type leading */
    string /* 13 return type trailing */

// ===== Trivia in wrapping function type with many params (line comments) =====
type TriviaLongFnType =
    ( // 1 trailing
    // 2 leading
    first_parameter // 3 trailing
    : // 4 trailing
    // 5 leading
    map<string, int> // 6 trailing
    , // 7 trailing
    // 8 leading
    second_parameter // 9 trailing
    : // 10 trailing
    // 11 leading
    (int | string)[] // 12 trailing
    , // 13 trailing
    // 14 leading
    third_parameter // 15 trailing
    : // 16 trailing
    // 17 leading
    map<string, bool> // 18 trailing
    ) // 19 trailing
    // 20 leading
    -> // 21 trailing
    // 22 leading
    string // 23 trailing

// ===== Trivia in optional type (line comments) =====
type TriviaOptional =
    int // 1 base type trailing
    // 2 question mark leading
    ? // 3 question mark trailing

// ===== Trivia in array type (line comments) =====
type TriviaArray =
    int // 1 base type trailing
    // 2 open bracket leading
    [ // 3 open bracket trailing
    // 4 close bracket leading
    ] // 5 close bracket trailing

// ===== Trivia in parenthesized type (line comments) =====
type TriviaParen =
    // 1 open paren leading
    ( // 2 open paren trailing
    // 3 inner leading
    int // 4 inner trailing
    | // 5 pipe trailing
    // 6 second member leading
    string // 7 second member trailing
    // 8 close paren leading
    ) // 9 close paren trailing
    // 10 array bracket leading
    [] // 11 array trailing

// ===== Short name =====
type T = int

// ===== Extraneous whitespace =====
type    ExtraWhitespace    =    int   |   string   |   bool   |   float   |   null
