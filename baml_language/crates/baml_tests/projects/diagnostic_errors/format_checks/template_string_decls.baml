// Formatter coverage migrated from template_string declarations to functions
// returning backtick strings.

function BasicTemplate(name: string) -> string {
    `Hello, ${name}!`
}

function NoParams() -> string {
    `A static template with no parameters.`
}

function MultiParam(first: string, last: string, age: int) -> string {
    `
        Name: ${first} ${last}
        Age: ${age}
    `
}

function ComplexParams(items: string[], config: map<string, int>) -> string {
    `
        Items: ${items}
        Config: ${config}
    `
}

function SingleLine(x: int) -> string { `Value is ${x}` }

function ManyParams(first_parameter: string, second_parameter: string, third_parameter: int, fourth_parameter: bool) -> string {
    `${first_parameter} ${second_parameter} ${third_parameter} ${fourth_parameter}`
}

function ComplexTypesWrap(items: (int | string | bool)[], config: map<string, int | string>, callback: (x: int, y: int) -> string) -> string {
    `${items} ${config}`
}

function OptionalParams(name: string?, age: int?, items: string[]?) -> string {
    `${name} ${age} ${items}`
}

function FnTypeParams(transform: (x: int) -> int, combine: (a: string, b: string) -> string) -> string {
    `transformed`
}

// Trivia around a function returning a backtick string.
function // keyword trailing
TriviaTemplate // name trailing
( // open paren trailing
    x // parameter trailing
    : string
) // close paren trailing
-> string {
    `Hello ${x}`
}

function WithControl(items: string[]) -> string {
    `
        ${for (let item in items)}
        - ${item}
        ${endfor}
    `
}

function WithConditional(show: bool, value: string) -> string {
    `${if (show)}${value}${else}hidden${endif}`
}
