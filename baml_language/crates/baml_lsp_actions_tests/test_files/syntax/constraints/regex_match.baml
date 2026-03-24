// Ensure no error or warning.

class PhoneNumber {
  value string @check(valid_phone_number, {{this|regex_match("\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}")}})
  foo int? // A nullable marker indicating PhoneNumber was chosen.
}

class EmailAddress {
  value string @check(valid_email, {{this|regex_match("^[_]*([a-z0-9]+(\.|_*)?)+@([a-z][a-z0-9-]+(\.|-*\.))+[a-z]{2,6}$")}})
  bar int? // A nullable marker indicating EmailAddress was chosen.
}

class ContactInfo {
  primary PhoneNumber | EmailAddress
}

//----
//- diagnostics
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
