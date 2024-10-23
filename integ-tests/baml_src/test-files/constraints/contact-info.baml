class PhoneNumber {
  value string @check(valid_phone_number, {{this|regex_match("\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}")}})
}

class EmailAddress {
  value string @check(valid_email, {{this|regex_match("^[_]*([a-z0-9]+(\.|_*)?)+@([a-z][a-z0-9-]+(\.|-*\.))+[a-z]{2,6}$")}})
}

class ContactInfo {
  primary PhoneNumber | EmailAddress
  secondary (PhoneNumber | EmailAddress)?
}

function ExtractContactInfo(document: string) -> ContactInfo {
  client GPT35
  prompt #"
         Extract a primary contact info, and if possible a secondary contact
         info, from this document:

         {{ document }}

         {{ ctx.output_format }}
  "#
}
