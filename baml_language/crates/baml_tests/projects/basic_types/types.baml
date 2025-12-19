class User {
    id int
    name string
    email string
}

enum Status {
    Active
    Inactive
}

// String literal type tests
type Role = "user" | "assistant" | "system"

// Optional string literal - regression test for from_type_text
type OptionalRole = "admin"?

// Array of string literal
type RoleList = "reader"[]

// Optional array of string literal
type OptionalRoleList = "writer"[]?

// Optional int literal
type OptionalCode = 200?

// Optional bool literal  
type OptionalFlag = true?