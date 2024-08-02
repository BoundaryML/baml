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
  client GPT4
  prompt #"
    Given the email below:

    ```
    from: {{email.from_address}}
    Email Subject: {{email.subject}}
    Email Body: {{email.body}}
    ```

    Extract this info from the email in JSON format:
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


