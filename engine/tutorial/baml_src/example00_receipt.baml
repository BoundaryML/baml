client<llm> GPT4o {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

class Receipt {
  establishment_name string
  total int @description("The total amount of the receipt")
  currency string
  items Item[] @description("The items on the receipt")
}

class Item {
  name string
  price float
  quantity int @description("If not specified, assume 1")
}

function ExtractReceipt(receipt: image | string) -> Receipt {
  client GPT4o
  prompt #"
    {{ _.role("user") }}

    Extract info from this receipt:
    {{ receipt }}

    {{ ctx.output_format }}
  "#
}

test ImageReceiptTest {
  functions [ExtractReceipt]
  args {
    receipt { url "https://i.reddit.it/adzt4bz4llfc1.jpeg" }
  }
}
