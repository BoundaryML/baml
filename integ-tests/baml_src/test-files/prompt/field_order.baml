class MaintainFieldOrder {
  a string
  b string
  c string
}

function UseMaintainFieldOrder(input: MaintainFieldOrder) -> MaintainFieldOrder {
  client GPT35
  prompt #"
    Return this value back to me: {{input}}
  "#
}


