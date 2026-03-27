template_string Greeting(name: string) #"
    Hello, {{ name }}!
"#

template_string NoParams() #"
    A static template.
"#

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// template_string.baml:1:1 (keyword) len=15 "template_string"
// template_string.baml:1:17 (function) len=8 "Greeting"
// template_string.baml:1:26 (parameter) len=4 "name"
// template_string.baml:1:32 (type) len=6 "string"
// template_string.baml:1:40 (string) len=1 "#"
// template_string.baml:1:41 (string) len=1 "\""
// template_string.baml:2:5 (string) len=5 "Hello"
// template_string.baml:2:10 (string) len=1 ","
// template_string.baml:2:12 (string) len=1 "{"
// template_string.baml:2:13 (string) len=1 "{"
// template_string.baml:2:15 (string) len=4 "name"
// template_string.baml:2:20 (string) len=1 "}"
// template_string.baml:2:21 (string) len=1 "}"
// template_string.baml:2:22 (string) len=1 "!"
// template_string.baml:3:1 (string) len=1 "\""
// template_string.baml:3:2 (string) len=1 "#"
// template_string.baml:5:1 (keyword) len=15 "template_string"
// template_string.baml:5:17 (function) len=8 "NoParams"
// template_string.baml:5:28 (string) len=1 "#"
// template_string.baml:5:29 (string) len=1 "\""
// template_string.baml:6:5 (string) len=1 "A"
// template_string.baml:6:7 (string) len=6 "static"
// template_string.baml:6:14 (string) len=8 "template"
// template_string.baml:6:22 (string) len=1 "."
// template_string.baml:7:1 (string) len=1 "\""
// template_string.baml:7:2 (string) len=1 "#"
