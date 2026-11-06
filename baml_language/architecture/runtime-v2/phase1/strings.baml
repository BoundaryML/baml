class TextBox { value: string }
function sliced(value: string) -> string {
  value.slice(end = -1, start = -3) + "!"
}
function main() -> int {
  let box = TextBox { value: sliced("aé🦀z") };
  box.value.length() * 100 + "aé🦀z".slice(-99, 99).length() * 10 + "aé🦀z".slice(3, 1).length()
}
