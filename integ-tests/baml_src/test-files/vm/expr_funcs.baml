function ReturnOne() -> int {
  1
}

function ReturnNumber(n: int) -> int {
  n
}

function CallReturnOne() -> int {
  ReturnOne()
}

function ChainedCalls() -> int {
  ReturnNumber(CallReturnOne())
}

function LlmReturnNumber(n: int) -> int {
  client GPT4o
  prompt #"
    Return the number {{ n }} without any additional text.
  "#
}

function CallLlmReturnNumber(n: int) -> int {
  LlmReturnNumber(n)
}
