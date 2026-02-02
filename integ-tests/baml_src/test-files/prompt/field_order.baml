class MaintainFieldOrder {
  c string
  b string
  a string
}

function UseMaintainFieldOrder(input: MaintainFieldOrder) -> MaintainFieldOrder {
  client GPT35
  prompt #"
    Return this value back to me: {{input}}
  "#
}


