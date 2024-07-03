// BAML excels at parsing the schemas from LLM responses.

// In this example we tell the model to think step by step and explain its reasoning
// before outputting the answer.

// Run this prompt, and you'll notice the BAML function still returns the expected return type (OrderInfo),
// even though there is a bunch of "reasoning" text present in the "raw text". It works with streaming or even standalone enum outputs too!
class Email {
    subject string
    body string
    from_address string
}

enum OrderStatus {
    ORDERED
    SHIPPED
    DELIVERED
    CANCELLED
}

class OrderInfo {
    order_status OrderStatus
    tracking_number string?
    estimated_arrival_date string?
}

function GetOrderInfo(email: Email) -> OrderInfo {
  client GPT4o
  prompt #"
    Extract the info from this email in the INPUT:

    INPUT:
    -------
    from: {{email.from_address}}
    Email Subject: {{email.subject}}
    Email Body: {{email.body}}
    -------

    {{ ctx.output_format }}

    Before you output the JSON, please explain your
    reasoning step-by-step. Here is an example on how to do this:
    'If we think step by step we can see that ...
     therefore the output JSON is:
    {
      ... the json schema ...
    }'
  "#
}

test Test1 {
  functions [GetOrderInfo]
  args {
    email {
      from_address "hello@amazon.com"
      subject "Your Amazon.com order of 'Wood Dowel Rods...' has shipped!"
      body #"
        Hi Sam, your package will arrive:
        Thurs, April 4
        Track your package:
        www.amazon.com/gp/your-account/ship-track?ie=23&orderId123

        On the way:
        Wood Dowel Rods...
        Order #113-7540940
        Ship to:
            Sam
            SEATTLE, WA

        Shipment total:
        $0.00
    "#

    }
  }
}