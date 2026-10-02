function basic_calls(
  flag: bool,
  count: int,
  ratio: float,
  text: string,
  nullable: string?,
) -> string {
  text
}

// Matches the shared function_calls fixture: omission, null, and invocation
// controls must remain independent at the generated call boundary.
function make_opt2() -> int { 99 }

function optional_args_probe(arg0: int, opt1: int? = 5, opt2: int? = make_opt2()) -> int?[] {
  [arg0, opt1, opt2]
}
