function V2_TestFnNamedArgsSyntax(var: string,  var_with_underscores: string) -> string {
  client GPT35
  prompt #"prompt"#
}
// TODO: we don't support numbers in named args yet!
// TODO: we also allow dashes but python fails.
