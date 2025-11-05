// Test raw string parsing in prompts and descriptions
class RawStrings {
  simple string @description(#"raw string"#)
  with_quotes string @description(#"contains "quotes" easily"#)
  multi_hash string @description(##"contains #"hash"# inside"##)
  multiline string @description(###"
    This is a multiline
    raw string with "quotes"
    and #hashes#
  "###)
}

function ProcessText(input string) -> string {
  client GPT4
  prompt #"
    Process the following text:
    {{ input }}
    
    Return the processed result.
  "#
}
