// Testing all the formatter cases for template string declarations
// as well as any special cases inside the template string

// ===== Basic template string =====
template_string BasicTemplate(name: string) #"
    Hello, {{ name }}!
"#

// ===== Template string with no params =====
template_string NoParams() #"
    A static template with no parameters.
"#

// ===== Template string with multiple params =====
template_string MultiParam(first: string, last: string, age: int) #"
    Name: {{ first }} {{ last }}
    Age: {{ age }}
"#

// ===== Template string with complex param types =====
template_string ComplexParams(items: string[], config: map<string, int>) #"
    Items: {{ items }}
    Config: {{ config }}
"#

// ===== Template string with single-line body =====
template_string SingleLine(x: int) #"Value is {{ x }}"#

// ===== Template string with many params that should wrap =====
template_string ManyParams(first_parameter: string, second_parameter: string, third_parameter: int, fourth_parameter: bool) #"
    {{ first_parameter }} {{ second_parameter }} {{ third_parameter }} {{ fourth_parameter }}
"#

// ===== Very long param list =====
template_string VeryLongParams(extremely_long_parameter_name: string, another_very_long_parameter_name: int) #"
    {{ extremely_long_parameter_name }} {{ another_very_long_parameter_name }}
"#

// ===== Template with template comments =====
template_string WithComment(x: string) #"
    {# This is a template comment #}
    Value: {{ x }}
"#

// ===== Trivia on template string declaration (line comments) =====
// 1 keyword leading
template_string // 2 keyword trailing
// 3 name leading
TriviaTemplate // 4 name trailing
// 5 open paren leading
( // 6 open paren trailing
// 7 param leading
x // 8 param name trailing
// 9 colon leading
:  // 10 colon trailing
// 11 type leading
string // 12 type trailing
// 13 close paren leading
) // 14 close paren trailing
// 15 body leading
#"
    Hello {{ x }}
"#

///////////////////

// ===== Trivia on template string declaration (block comments) =====
/* 1 keyword leading */
template_string /* 2 keyword trailing */
/* 3 name leading */
TriviaTemplateBlock /* 4 name trailing */
/* 5 open paren leading */
( /* 6 open paren trailing */
/* 7 param leading */
x /* 8 param name trailing */
: /* 9 colon trailing */
/* 10 type leading */
string /* 11 type trailing */
/* 12 close paren leading */
) /* 13 close paren trailing */
/* 14 body leading */
#"
    Hello {{ x }}
"#

///////////////////

// ===== Multi param trivia (line comments) =====
template_string MultiParamTrivia(
    // 1 first param leading
    a // 2 first param trailing
    : // 3 first colon trailing
    // 4 first type leading
    string // 5 first type trailing
    , // 6 comma trailing
    // 7 second param leading
    b // 8 second param trailing
    : // 9 second colon trailing
    // 10 second type leading
    int // 11 second type trailing
) #"
    {{ a }} {{ b }}
"#

// ===== Multi param trivia (block comments) =====
template_string MultiParamTriviaBlock(
    /* 1 first param leading */
    a /* 2 first param trailing */
    : /* 3 first colon trailing */
    /* 4 first type leading */
    string /* 5 first type trailing */
    , /* 6 comma trailing */
    /* 7 second param leading */
    b /* 8 second param trailing */
    : /* 9 second colon trailing */
    /* 10 second type leading */
    int /* 11 second type trailing */
) #"
    {{ a }} {{ b }}
"#

// ===== Short name =====
template_string T(x: int) #"{{ x }}"#

// ===== Long name exceeding limit =====
template_string ThisIsAnExtremelyLongTemplateStringNameThatDefinitelyExceedsTheDefaultLineLimitOfOneHundredColumns(x: int) #"
    {{ x }}
"#

// ===== Long name + many params exceeding limit =====
template_string VeryLongTemplateNameWithManyParameters(first_parameter: string, second_parameter: int, third_parameter: bool, fourth_parameter: float) #"
    {{ first_parameter }} {{ second_parameter }} {{ third_parameter }} {{ fourth_parameter }}
"#

// ===== Many params with complex types exceeding limit =====
template_string ComplexTypesWrap(items: (int | string | bool)[], config: map<string, int | string>, callback: (x: int, y: int) -> string) #"
    {{ items }} {{ config }}
"#

// ===== Very long param list with many complex params far exceeding limit =====
template_string VeryLongComplexParams(first_parameter: map<string, int | string | bool>, second_parameter: (int | string)[], third_parameter: map<string, map<string, int>>, fourth_parameter: (x: int) -> string) #"
    {{ first_parameter }} {{ second_parameter }} {{ third_parameter }}
"#

// ===== Params with deeply nested generic types exceeding limit =====
template_string DeepGenericParams(data: map<string, map<string, map<string, int | string | bool | float | null>>>) #"
    {{ data }}
"#

// ===== Params with constrained types =====
template_string ConstrainedParams(value: int @check("positive", {{ this > 0 }}), name: string) #"
    {{ value }} {{ name }}
"#

// ===== Params with union types =====
template_string UnionParams(result: string | int | bool | null, items: (string | int)[]) #"
    {{ result }} {{ items }}
"#

// ===== Params with optional types =====
template_string OptionalParams(name: string?, age: int?, items: string[]?) #"
    {{ name }} {{ age }} {{ items }}
"#

// ===== Params with function types =====
template_string FnTypeParams(transform: (x: int) -> int, combine: (a: string, b: string) -> string) #"
    transformed
"#

// ===== Long name + long complex params far exceeding limit =====
template_string ExtremelyLongNamePlusComplexParameters(first_very_long_parameter_name: map<string, int | string | bool>, second_very_long_parameter_name: (int | string | bool | float | null)[]) #"
    {{ first_very_long_parameter_name }} {{ second_very_long_parameter_name }}
"#

// ===== Six params all with long names and types exceeding limit =====
template_string SixLongParams(first_param: string, second_param: int, third_param: bool, fourth_param: float, fifth_param: string[], sixth_param: map<string, int>) #"
    {{ first_param }} {{ second_param }} {{ third_param }} {{ fourth_param }} {{ fifth_param }} {{ sixth_param }}
"#

// ===== Trivia on wrapping many params (line comments) =====
// 1 keyword leading
template_string // 2 keyword trailing
// 3 name leading
TriviaWrappingParams // 4 name trailing
// 5 open paren leading
( // 6 open paren trailing
// 7 first param leading
first_parameter // 8 first name trailing
: // 9 first colon trailing
// 10 first type leading
map<string, int | string | bool> // 11 first type trailing
, // 12 comma trailing
// 13 second param leading
second_parameter // 14 second name trailing
: // 15 second colon trailing
// 16 second type leading
(int | string)[] // 17 second type trailing
, // 18 comma trailing
// 19 third param leading
third_parameter // 20 third name trailing
: // 21 third colon trailing
// 22 third type leading
map<string, map<string, int>> // 23 third type trailing
// 24 close paren leading
) // 25 close paren trailing
// 26 body leading
#"
    {{ first_parameter }} {{ second_parameter }} {{ third_parameter }}
"#

///////////////////

// ===== Trivia on wrapping many params (block comments) =====
/* 1 keyword leading */
template_string /* 2 keyword trailing */
/* 3 name leading */
TriviaWrappingParamsBlock /* 4 name trailing */
/* 5 open paren leading */
( /* 6 open paren trailing */
/* 7 first param leading */
first_parameter /* 8 first name trailing */
: /* 9 first colon trailing */
/* 10 first type leading */
map<string, int | string | bool> /* 11 first type trailing */
, /* 12 comma trailing */
/* 13 second param leading */
second_parameter /* 14 second name trailing */
: /* 15 second colon trailing */
/* 16 second type leading */
(int | string)[] /* 17 second type trailing */
, /* 18 comma trailing */
/* 19 third param leading */
third_parameter /* 20 third name trailing */
: /* 21 third colon trailing */
/* 22 third type leading */
map<string, map<string, int>> /* 23 third type trailing */
/* 24 close paren leading */
) /* 25 close paren trailing */
/* 26 body leading */
#"
    {{ first_parameter }} {{ second_parameter }} {{ third_parameter }}
"#

///////////////////

// ===== Trivia on wrapping long name + params (line comments) =====
// 1 keyword leading
template_string // 2 keyword trailing
// 3 name leading
TriviaVeryLongTemplateNameWithParameters // 4 name trailing
// 5 open paren leading
( // 6 open paren trailing
// 7 first param leading
first_parameter // 8 first name trailing
: // 9 first colon trailing
// 10 first type leading
string // 11 first type trailing
, // 12 comma trailing
// 13 second param leading
second_parameter // 14 second name trailing
: // 15 second colon trailing
// 16 second type leading
int // 17 second type trailing
, // 18 comma trailing
// 19 third param leading
third_parameter // 20 third name trailing
: // 21 third colon trailing
// 22 third type leading
bool // 23 third type trailing
// 24 close paren leading
) // 25 close paren trailing
// 26 body leading
#"
    {{ first_parameter }} {{ second_parameter }} {{ third_parameter }}
"#

///////////////////

// ===== Trivia on wrapping long name + params (block comments) =====
/* 1 keyword leading */
template_string /* 2 keyword trailing */
/* 3 name leading */
TriviaVeryLongTemplateNameWithParametersBlock /* 4 name trailing */
/* 5 open paren leading */
( /* 6 open paren trailing */
/* 7 first param leading */
first_parameter /* 8 first name trailing */
: /* 9 first colon trailing */
/* 10 first type leading */
string /* 11 first type trailing */
, /* 12 comma trailing */
/* 13 second param leading */
second_parameter /* 14 second name trailing */
: /* 15 second colon trailing */
/* 16 second type leading */
int /* 17 second type trailing */
, /* 18 comma trailing */
/* 19 third param leading */
third_parameter /* 20 third name trailing */
: /* 21 third colon trailing */
/* 22 third type leading */
bool /* 23 third type trailing */
/* 24 close paren leading */
) /* 25 close paren trailing */
/* 26 body leading */
#"
    {{ first_parameter }} {{ second_parameter }} {{ third_parameter }}
"#

///////////////////

// ===== Trivia on param with deeply nested generic type (line comments) =====
template_string TriviaDeepGenericParam(
    // 1 param leading
    data // 2 name trailing
    : // 3 colon trailing
    // 4 type leading
    map // 5 map trailing
    < // 6 open angle trailing
    // 7 key type leading
    string // 8 key type trailing
    , // 9 comma trailing
    // 10 value type leading
    map // 11 inner map trailing
    < // 12 inner open angle trailing
    // 13 inner key leading
    string // 14 inner key trailing
    , // 15 inner comma trailing
    // 16 inner value leading
    int // 17 inner value trailing
    | // 18 pipe trailing
    // 19 union member leading
    string // 20 union member trailing
    > // 21 inner close angle trailing
    > // 22 outer close angle trailing
) #"
    {{ data }}
"#

// ===== Trivia on param with deeply nested generic type (block comments) =====
template_string TriviaDeepGenericParamBlock(
    /* 1 param leading */
    data /* 2 name trailing */
    : /* 3 colon trailing */
    /* 4 type leading */
    map /* 5 map trailing */
    < /* 6 open angle trailing */
    /* 7 key type leading */
    string /* 8 key type trailing */
    , /* 9 comma trailing */
    /* 10 value type leading */
    map /* 11 inner map trailing */
    < /* 12 inner open angle trailing */
    /* 13 inner key leading */
    string /* 14 inner key trailing */
    , /* 15 inner comma trailing */
    /* 16 inner value leading */
    int /* 17 inner value trailing */
    | /* 18 pipe trailing */
    /* 19 union member leading */
    string /* 20 union member trailing */
    > /* 21 inner close angle trailing */
    > /* 22 outer close angle trailing */
) #"
    {{ data }}
"#

// ===== Extraneous whitespace =====
template_string    ExtraWhitespace   (   a   :   string   ,   b   :   int   )   #"
    {{ a }} {{ b }}
"#

// ===== Template with control flow =====
template_string WithControl(items: string[]) #"
    {% for item in items %}
    - {{ item }}
    {% endfor %}
"#
