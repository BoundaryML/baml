
class Receipt {
  establishment_name string
  date string @description(#"
    ISO8601 formatted date
  "#)
  total int @description(#"
    The total amount of the receipt
  "#)
  currency string
  items Item[] @description(#"
    The items on the receipt
  "#)
}

class Item {
  name string
  price float
  quantity int
}

// Input can be an image or text here
function ExtractReceipt(receipt: image | string) -> Receipt {
  client GPT4o
  prompt #"
    {{ _.role("user") }}

    Extract info from this receipt:
    {{ receipt }}

    {# special macro to print the output schema instructions. #}
    {{ ctx.output_format }}
  "#
}

test ImageReceiptTest {
  functions [ExtractReceipt]
  args {
    receipt { url "https://i.redd.it/adzt4bz4llfc1.jpeg"}
  }
}

test StarbucksTextReceiptTest {
  functions [ExtractReceipt]
  args {
    receipt #"
      Starbucks
      Date: 2022-01-01
      Total: $5.00 USD
      Items:
      - Coffee
        - $2.50
        - 1
      - Croissant
        - $2.50
        - 1
    "#
  }
}
