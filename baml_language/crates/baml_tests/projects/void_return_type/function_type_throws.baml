type ExplicitHandler = (value: int) -> string throws never

type ImplicitHandler = (value: int) -> string

type HiddenHandler = (value: int) -> string

function run_explicit(handler: ExplicitHandler, value: int) -> string {
  handler(value)
}

function run_implicit(handler: ImplicitHandler, value: int) -> string {
  handler(value)
}

function forward_direct(handler: (value: int) -> string, value: int) -> string {
  handler(value)
}

function forward_optional(handler: ((value: int) -> string)?, value: int) -> string? {
  handler?.(value)
}

function run_hidden(handler: HiddenHandler, value: int) -> string {
  handler(value)
}

function return_closure(handler: (value: int) -> string) -> (value: int) -> string {
  return (value: int) -> string { handler(value) }
}
