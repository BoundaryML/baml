// docs:start types
class ReceiptItem {
  name: string,
  quantity: int,
  price: float,
}

class Receipt {
  items: ReceiptItem[],
  total: float?,
}
// docs:end types

// docs:start function
function extract_receipt(text: string) -> Receipt {
  client: openai.ResponsesClient.new(model = "gpt-4.1-mini", api_key = env.OPENAI_API_KEY)
  prompt: `
    ${role("system")}
    Extract the items and total from the receipt. Use null if the total is missing.
    ${ctx.output_format()}

    ${role("user")}
    ${text}
  `
}
// docs:end function
