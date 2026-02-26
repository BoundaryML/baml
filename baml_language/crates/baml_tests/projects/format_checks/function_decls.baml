// Testing all the formatter cases for function declarations
// both expression functions and llm functions

// ===== Minimal expression function =====
function Minimal() -> int {
    1
}

// ===== Single param =====
function SingleParam(x: int) -> int {
    x
}




// ===== Multiple params =====
function MultipleParams(a: int, b: int, c: string) -> int {
    a + b
}

// ===== Params without colons =====
function NoColonParams(x int, y int) -> int {
    x + y
}

// ===== Many params wrapping =====
function ManyParams(first_parameter: int, second_parameter: string, third_parameter: bool, fourth_parameter: float) -> string {
    "result"
}

// ===== Very long param list =====
function VeryLongParamList(extremely_long_parameter_name: int, another_very_long_parameter_name: string, yet_another_long_one: bool) -> string {
    "result"
}





// ===== Six params with long names and types =====
function SixLongParams(first_parameter_name: map<string, int>, second_parameter_name: (int | string)[], third_parameter_name: map<string, bool>, fourth_parameter_name: int, fifth_parameter_name: string, sixth_parameter_name: float) -> string {
    "result"
}

// ===== Long name alone exceeding limit =====
function ThisIsAnExtremelyLongFunctionNameThatDefinitelyCausesWrappingAllByItselfWithNoParamsNeeded() -> int {
    1
}

// ===== Long name + params =====
function LongNameWithManyParameters_ThisFunctionExceedsLimit(first_argument: string, second_argument: int, third_argument: bool) -> string {
    "result"
}

// ===== Long name + long return type =====
function LongNameWithLongReturnType_ExceedsTheLineLimit(x: int) -> map<string, map<string, int | string | bool | float | null>> {
    {"a": {"b": 1}}
}

// ===== Long name + long params + long return =====
function EverythingIsLong_NameParamsReturnType(first_long_param: map<string, int>, second_long_param: (int | string | bool)[]) -> map<string, int | string | bool | float | null> {
    {"a": 1}
}

// ===== Complex return type =====
function ComplexReturn(x: int) -> int | string | null {
    x
}

// ===== Return type: nested generics exceeding limit =====
function NestedGenericReturn() -> map<string, map<string, map<string, int | string | bool | null | float>>> {
    {"a": {"b": {"c": 1}}}
}

// ===== Return type: deeply nested function type exceeding limit =====
function DeepFnReturn() -> (x: int) -> (y: int) -> (z: int) -> (w: int) -> map<string, int | string | bool> {
    1
}

// ===== Deeply nested param types =====
function DeepParamTypes(nested: map<string, map<string, map<string, int[]>>>, callback: (x: int) -> (y: int) -> (z: int) -> string) -> string {
    "done"
}




// ===== Params with constrained types exceeding limit =====
function ConstrainedParams(name: string, score: int @check({{ this > 0 }})) -> string {
    "done"
}



// ===== Trivia on function declaration (line comments) =====
// 1 function keyword leading
function // 2 function keyword trailing
// 3 name leading
TriviaFunc // 4 name trailing
// 5 open paren leading
( // 6 open paren trailing
// 7 param leading
x // 8 param name trailing
: // 9 colon trailing
// 10 type leading
int // 11 type trailing
// 12 close paren leading
) // 13 close paren trailing
// 14 arrow leading
-> // 15 arrow trailing
// 16 return type leading
int // 17 return type trailing
// 18 body leading
{ // 19 body open trailing
    x
} // 20 body close trailing

///////////////////

// ===== Trivia on function declaration (block comments) =====
/* 1 function keyword leading */
function /* 2 function keyword trailing */
/* 3 name leading */
TriviaFuncBlock /* 4 name trailing */
/* 5 open paren leading */
( /* 6 open paren trailing */
/* 7 param leading */
x /* 8 param name trailing */
: /* 9 colon trailing */
/* 10 type leading */
int /* 11 type trailing */
/* 12 close paren leading */
) /* 13 close paren trailing */
/* 14 arrow leading */
-> /* 15 arrow trailing */
/* 16 return type leading */
int /* 17 return type trailing */
/* 18 body leading */
{ /* 19 body open trailing */
    x
} /* 20 body close trailing */

///////////////////

// ===== Multiple param trivia (line comments) =====
function MultiParamTrivia(
    // 1 first param leading
    a // 2 first param trailing
    : // 3 first colon trailing
    // 4 first type leading
    int // 5 first type trailing
    , // 6 comma trailing
    // 7 second param leading
    b // 8 second param trailing
    : // 9 second colon trailing
    // 10 second type leading
    string // 11 second type trailing
) -> int {
    a
}

// ===== Multiple param trivia (block comments) =====
function MultiParamTriviaBlock(
    /* 1 first param leading */
    a /* 2 first param trailing */
    : /* 3 first colon trailing */
    /* 4 first type leading */
    int /* 5 first type trailing */
    , /* 6 comma trailing */
    /* 7 second param leading */
    b /* 8 second param trailing */
    : /* 9 second colon trailing */
    /* 10 second type leading */
    string /* 11 second type trailing */
) -> int {
    a
}



// ===== Trivia on wrapping function with many params (line comments) =====
// 1 leading
function // 2 trailing
// 3 leading
TriviaWrapParams // 4 trailing
( // 5 trailing
// 6 leading
first_parameter // 7 trailing
: // 8 trailing
// 9 leading
int // 10 trailing
, // 11 trailing
// 12 leading
second_parameter // 13 trailing
: // 14 trailing
// 15 leading
string // 16 trailing
, // 17 trailing
// 18 leading
third_parameter // 19 trailing
: // 20 trailing
// 21 leading
bool // 22 trailing
, // 23 trailing
// 24 leading
fourth_parameter // 25 trailing
: // 26 trailing
// 27 leading
float // 28 trailing
) // 29 trailing
// 30 leading
-> // 31 trailing
// 32 leading
string // 33 trailing
// 34 leading
{ // 35 trailing
    "result"
} // 36 trailing

///////////////////

// ===== Trivia on wrapping function with many params (block comments) =====
/* 1 leading */
function /* 2 trailing */
/* 3 leading */
TriviaWrapParamsBlock /* 4 trailing */
( /* 5 trailing */
/* 6 leading */
first_parameter /* 7 trailing */
: /* 8 trailing */
/* 9 leading */
int /* 10 trailing */
, /* 11 trailing */
/* 12 leading */
second_parameter /* 13 trailing */
: /* 14 trailing */
/* 15 leading */
string /* 16 trailing */
, /* 17 trailing */
/* 18 leading */
third_parameter /* 19 trailing */
: /* 20 trailing */
/* 21 leading */
bool /* 22 trailing */
, /* 23 trailing */
/* 24 leading */
fourth_parameter /* 25 trailing */
: /* 26 trailing */
/* 27 leading */
float /* 28 trailing */
) /* 29 trailing */
/* 30 leading */
-> /* 31 trailing */
/* 32 leading */
string /* 33 trailing */
/* 34 leading */
{ /* 35 trailing */
    "result"
} /* 36 trailing */

///////////////////

// ===== Trivia on wrapping return type (line comments) =====
function TriviaWrapReturnType(x: int) ->
    // 1 first member leading
    int // 2 first trailing
    // 3 pipe leading
    | // 4 pipe trailing
    // 5 second leading
    string // 6 second trailing
    // 7 pipe leading
    | // 8 pipe trailing
    // 9 third leading
    bool // 10 third trailing
    // 11 pipe leading
    | // 12 pipe trailing
    // 13 fourth leading
    null // 14 fourth trailing
{
    x
}



// ===== Trivia on wrapping return type (block comments) =====
function TriviaWrapReturnTypeBlock(x: int) ->
    /* 1 first member leading */
    int /* 2 first trailing */
    /* 3 pipe leading */
    | /* 4 pipe trailing */
    /* 5 second leading */
    string /* 6 second trailing */
    /* 7 pipe leading */
    | /* 8 pipe trailing */
    /* 9 third leading */
    bool /* 10 third trailing */
    /* 11 pipe leading */
    | /* 12 pipe trailing */
    /* 13 fourth leading */
    null /* 14 fourth trailing */
{
    x


}

///////////////////

// ===== Trivia on wrapping long-name + params + return (line comments) =====
// 1 leading
function // 2 trailing
// 3 leading
TriviaEverythingLong_NameAndParamsAndReturn // 4 trailing
( // 5 trailing
// 6 leading
first_long_param // 7 trailing
: // 8 trailing
// 9 leading
map<string, int> // 10 trailing
, // 11 trailing
// 12 leading
second_long_param // 13 trailing
: // 14 trailing
// 15 leading
(int | string | bool)[] // 16 trailing
) // 17 trailing
// 18 leading
-> // 19 trailing
// 20 leading
map<string, int | string | bool | float | null> // 21 trailing
// 22 leading
{ // 23 trailing
    {"a": 1}
} // 24 trailing

///////////////////

// ===== Trivia on wrapping long-name + params + return (block comments) =====
/* 1 leading */
function /* 2 trailing */
/* 3 leading */
TriviaEverythingLong_NameAndParamsAndReturnBlock /* 4 trailing */
( /* 5 trailing */
/* 6 leading */
first_long_param /* 7 trailing */
: /* 8 trailing */
/* 9 leading */
map<string, int> /* 10 trailing */
, /* 11 trailing */
/* 12 leading */
second_long_param /* 13 trailing */
: /* 14 trailing */
/* 15 leading */
(int | string | bool)[] /* 16 trailing */
) /* 17 trailing */
/* 18 leading */
-> /* 19 trailing */
/* 20 leading */
map<string, int | string | bool | float | null> /* 21 trailing */
/* 22 leading */
{ /* 23 trailing */
    {"a": 1}
} /* 24 trailing */

///////////////////

// ===== Expression function with multiple statements =====
function MultiStatement(a: int, b: int) -> int {
    let x = a + b;
    let y = x * 2;
    if (y > 10) {
        return y;
    }
    return x;
}

// ===== Expression function with final expression =====
function FinalExpr(a: int, b: int) -> int {
    a + b
}

// ===== Deeply nested if/else body =====
function DeeplyNestedBody(a: int, b: int, c: int) -> int {
    if (a > 0) {
        if (b > 0) {
            if (c > 0) {
                let x = a + b + c;
                if (x > 100) {
                    return x * 2;
                } else {
                    return x;
                }
            } else {
                return a + b;
            }
        } else {
            if (c > 0) {
                return a + c;
            } else {
                return a;
            }
        }
    } else {
        return 0;
    }
}

// ===== Many nested let bindings with if/match/block =====
function ManyNestedExprs(items: int[]) -> int {
    let a = items[0];
    let b = items[1];
    let c = if (a > b) { a } else { b };
    let d = match (c) {
        0 => 1,
        _ => c * 2
    };
    let e = {
        let inner = d + a;
        inner * inner
    };
    e + d + c + b + a
}

// ===== Deeply nested loops with breaks/continues =====
function NestedLoops(matrix: int[][]) -> int {
    let total = 0;
    for (let row in matrix) {
        for (let val in row) {
            if (val < 0) {
                continue;
            }
            if (val > 1000) {
                break;
            }
            let processed = match (val % 3) {
                0 => val * 2,
                1 => val + 1,
                _ => val
            };
            total += processed;
        };
    };
    return total;
}

// ===== Long expression at deep nesting pushing past limit =====
function DeepNestLongExpr(a: int, b: int, c: int, d: int) -> int {
    if (a > 0) {
        if (b > 0) {
            if (c > 0) {
                return a * b * c * d + a * b * c + a * b + a + b * c * d + b * c + b + c * d + c + d;
            } else {
                return 0;
            }
        } else {
            return 0;
        }
    } else {
        return 0;
    }
}

// ===== LLM function basic =====
function LlmBasic(name: string) -> string {
    client GPT4
    prompt #"Say hello to {{name}}"#
}






// ===== LLM function with multi-line prompt =====
function LlmMultiLine(text: string) -> string {
    client GPT4
    prompt #"
        Analyze the following text:
        {{ text }}
        Return a summary.
    "#
}

// ===== LLM function with string client =====
function LlmStringClient(text: string) -> string {
    client "openai/gpt-4o"
    prompt #"Summarize: {{ text }}"#
}

// ===== LLM function prompt before client (order test) =====
function LlmReversedOrder(text: string) -> string {
    prompt #"Summarize: {{ text }}"#
    client GPT4
}



// ===== LLM function with wrapping signature =====
function LlmWithLongSignature(first_context: string, second_context: string, third_context: string, user_query: string) -> string {
    client GPT4
    prompt #"
        Context: {{ first_context }} {{ second_context }} {{ third_context }}
        Query: {{ user_query }}
    "#
}

// ===== Extraneous whitespace on function =====
function    ExtraWhitespace   (   a   :   int   ,   b   :   int   )   ->   int   {
    a + b
}

// ===== Short function name =====
function F() -> int {
    1
}
