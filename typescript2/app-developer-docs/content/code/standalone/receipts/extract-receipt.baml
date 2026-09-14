// docs:start types
class ReceiptItem {
  name string
  quantity int
  price float
}

class Receipt {
  items ReceiptItem[]
  total float?
}
// docs:end types

// docs:start function
function ExtractReceipt(text: string) -> Receipt {
  client: "openai/gpt-4o-mini"
  prompt: `
    Extract the receipt from the following text.

    ${text}

    ${ctx.output_format()}
  `
}
// docs:end function
