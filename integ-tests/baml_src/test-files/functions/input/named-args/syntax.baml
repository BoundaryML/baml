function TestFnNamedArgsSyntax {
  input (myVar: string,  var_with_underscores: string)
  output string
}
// TODO: we don't support numbers in named args yet!
// TODO: we also allow dashes but python fails.