class ReceiptItem {
  name string
  description string?
  quantity int
  price float
}

class ReceiptInfo {
    items ReceiptItem[]
    total_cost float?
}

function ExtractReceiptInfo(email: string) -> ReceiptInfo {
  client GPT4o
  prompt #"
    Given the receipt below:

    ```
    {{email}}
    ```

    {{ ctx.output_format }}
  "#
}

