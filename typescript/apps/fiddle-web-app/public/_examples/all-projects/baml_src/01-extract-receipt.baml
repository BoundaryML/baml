// This is a BAML file, which extends the Jinja2 templating language to write LLM functions.
// Run a test to see how it works!

// https://docs.boundaryml.com

// We want the LLM to extract this info from an image receipt
class Receipt {
  establishment_name string
  date string @description("ISO8601 formatted date")
  total int @description("The total amount of the receipt")
  currency string
  items Item[] @description("The items on the receipt")
}

class Item {
  name string
  price float
  quantity int @description("If not specified, assume 1")
}
 
// This is our LLM function we can call in Python or Typescript
// the receipt can be an image OR text here!
function ExtractReceipt(receipt: image | string) -> Receipt {
  // see clients.baml
  client GPT4o
  prompt #"
    {# start a user message #}
    {{ _.role("user") }}

    Extract info from this receipt:
    {{ receipt }}

    {# special macro to print the output schema instructions. #}
    {{ ctx.output_format }}
  "#
}

// Test when the input is an image
test ImageReceiptTest {
  functions [ExtractReceipt]
  args {
    receipt { url "https://i.redd.it/adzt4bz4llfc1.jpeg"}
  }
}

// Test when the input is a string
test StarbucksTextReceiptTest {
  functions [ExtractReceipt]
  args {
    // use #""# for multi-line strings
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
