// Test: Valid single class description
class ValidClassDescription {
  id int
  name string
  @@description("A valid class description")
}

// Test: Valid multiline class description
class MultilineClassDescription {
  id int
  @@description(#"
    This class represents a complex entity
    with multiple responsibilities
  "#)
}

// Test: Class with both description and alias
class DescriptionWithAlias {
  value string
  @@description("An important class")
  @@alias("ImportantClass")
}

// Test: Both class and field descriptions
class UserWithDescriptions {
  name string @description("User's full name")
  email string @description("Contact email")
  @@description("Represents a user account in the system")
}
