template_string SomeString #"
  ...
"#

template_string SomeString(a: int) #"
  ...
"#

function SomeString {
  input string
  output string
}

// error: The template_string "SomeString" cannot be defined because a template_string with that name already exists.
//   -->  template_string/duplicate_names.baml:5
//    | 
//  4 | 
//  5 | template_string SomeString(a: int) #"
//    | 
// error: The function "SomeString" cannot be defined because a template_string with that name already exists.
//   -->  template_string/duplicate_names.baml:9
//    | 
//  8 | 
//  9 | function SomeString {
//    | 
