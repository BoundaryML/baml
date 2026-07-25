// Field attributes (`@description`, `@alias`) classified as decorators.
class Resume {
  name string @description("the candidate's full name")
  email string @alias("email_address")
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// attributes.baml:1:1 (comment) len=72 "// Field attributes (`@description`, `@alias`) classified as decorators."
// attributes.baml:2:1 (keyword) len=5 "class"
// attributes.baml:2:7 (class) [declaration] len=6 "Resume"
// attributes.baml:3:3 (property) [declaration] len=4 "name"
// attributes.baml:3:8 (type) [defaultLibrary] len=6 "string"
// attributes.baml:3:15 (decorator) len=1 "@"
// attributes.baml:3:16 (decorator) len=11 "description"
// attributes.baml:3:28 (string) len=27 "\"the candidate's full name\""
// attributes.baml:4:3 (property) [declaration] len=5 "email"
// attributes.baml:4:9 (type) [defaultLibrary] len=6 "string"
// attributes.baml:4:16 (decorator) len=1 "@"
// attributes.baml:4:17 (decorator) len=5 "alias"
// attributes.baml:4:23 (string) len=15 "\"email_address\""
