// --------------------------------
// Pure Expression Functions.
// --------------------------------

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

function StoreFnCallInLocalVar(n: int) -> int {
  let result = ReturnNumber(n);

  result
}

function BoolToIntWithIfElse(b: bool) -> int {
  if b { 1 } else { 0 }
}

// --------------------------------
// Expression Functions with LLM calls.
// --------------------------------

function LlmReturnNumber(n: int) -> int {
  client GPT35LegacyProvider // GPT 3.5 Turbo
  prompt #"
    Return the number {{ n }} without any additional text.
  "#
}

function ReturnNumberCallingLlm(n: int) -> int {
  LlmReturnNumber(n)
}

function StoreLlmCallInLocalVar(n: int) -> int {
  let result = LlmReturnNumber(n);

  result
}

function BoolToIntWithIfElseCallingLlm(b: bool) -> int {
  if b { LlmReturnNumber(1) } else { LlmReturnNumber(0) }
}
