class Document1559 {
  client_details ClientDetails1559
  notes Note1599[]
}

class ClientDetails1559 {
  client_name string?
  client_address string?
  client_postal_code string?
  client_city string?
  client_country string?
  client_phone string?
  client_email string?
}

class Note1599 {
  note_title string
  note_description string?
  note_amount string? @description("If there is a quantity, specify it here")
}

function DescribeMedia1599(img: image, client_sector: string, client_name: string) -> string {
  client "openai/gpt-4o-mini"
  prompt #"
    {{_.role("system")}}
    You are an expert at describing media.
    {{_.role("user")}}
    Describe this image {{img}} for client {{ client_name }} in sector {{ client_sector }}.
  "#
}

function StructureDocument1559(document_txt: string) -> Document1559 {
  client "openai/gpt-4o-mini"
  prompt #"
    {{_.role("system")}}
    You are an expert in structuring notes.
    {{_.role("user")}}
    Here is the text you need to structure:
    {{ document_txt }}

    {{ ctx.output_format }}
  "#
}

test TestDescribeMedia1559 {
  functions [DescribeMedia1599]
  args {
    img { file "./notes.png" }
    client_sector #"
      roofer
    "#
    client_name #"
      The Vroe Group
    "#
  }
}

test TestStructureDocument1559 {
  functions [StructureDocument1559]
  args {
    document_txt "hello"
  }
}
