require 'test/unit'
require 'baml'

# this test suite verifies that our type builder system correctly handles
# string representations of complex types like classes and enums. this is
# important for debugging and logging purposes, as it helps
# understand the structure of their type definitions at runtime.
class TypeBuilderTest < Test::Unit::TestCase

  # tests that class definitions are properly stringified with all their
  # properties and metadata intact. this helps ensure our type system
  # maintains semantic meaning when displayed to users.
  def test_class_string_representation
    # start with a fresh type builder - this is our main interface
    # for constructing type definitions programmatically
    builder = Baml::Ffi::TypeBuilder.new

    # create a new user class - this represents a person in our system
    # with various attributes that describe them
    user_class = builder.class_('User')

    # define the core properties that make up a user profile
    # we use aliases and descriptions to make the api more human-friendly
    user_class.property('name')
      .alias('username')  # allows 'username' as an alternative way to reference this
      .description('The user\'s full name')  # helps explain the purpose

    user_class.property('age')
      .description('User\'s age in years')  # clarifies the expected format

    user_class.property('email')  # sometimes a property name is self-explanatory

    # convert our type definition to a human-readable string
    # this is invaluable for debugging and documentation
    output = builder.to_s
    puts "\nClass output:\n#{output}\n"

    # verify that the string output matches our expectations
    # we check for key structural elements and metadata
    assert_match(/TypeBuilder\(\n  Classes: \[\n    User \{/, output)
    assert_match(/name \(unknown-type\) \(alias=String\("username"\), description=String\("The user's full name"\)\)/, output)
    assert_match(/age \(unknown-type\) \(description=String\("User's age in years"\)\)/, output)
    assert_match(/email \(unknown-type\)/, output)
  end

  # tests that enum definitions are correctly stringified with their
  # values and associated metadata. enums help us model fixed sets
  # of options in a type-safe way.
  def test_enum_string_representation
    # create a fresh builder for our enum definition
    builder = Baml::Ffi::TypeBuilder.new

    # define a status enum to track user account states
    # this gives us a type-safe way to handle different user situations
    status_enum = builder.enum('Status')

    # add the possible status values with helpful metadata
    # active users are currently using the system
    status_enum.value('ACTIVE')
      .alias('active')  # lowercase alias for more natural usage
      .description('User is active')  # explains the meaning

    # inactive users have temporarily stopped using the system
    status_enum.value('INACTIVE')
      .alias('inactive')

    # pending users are in a transitional state
    status_enum.value('PENDING')

    # generate a readable version of our enum definition
    output = builder.to_s
    puts "\nEnum output:\n#{output}\n"

    # verify the string representation includes all our carefully
    # defined values and their metadata
    assert_match(/TypeBuilder\(\n  Enums: \[\n    Status \{/, output)
    assert_match(/ACTIVE \(alias=String\("active"\), description=String\("User is active"\)\)/, output)
    assert_match(/INACTIVE \(alias=String\("inactive"\)\)/, output)
    assert_match(/PENDING/, output)
  end
end