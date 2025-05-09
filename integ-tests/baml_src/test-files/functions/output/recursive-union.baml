type RecursiveUnion = string | map<string, RecursiveUnion>

function RecursiveUnionTest(input: RecursiveUnion) -> RecursiveUnion {
  client GPT35
  prompt #"
    Return back the same value you were given: {{ input }}
    
    {{ ctx.output_format }}
  "#
}

test RecursiveUnionTest {
  functions [RecursiveUnionTest]
  args {
    input {
      "key" "value"
      "key2" {
        "key" "value2"
        "key2" "value3"
      }
    }
  }
}
