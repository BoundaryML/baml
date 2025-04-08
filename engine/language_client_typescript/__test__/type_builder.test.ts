// import the typescript wrapper for the type builder that provides a clean interface
// over the native rust implementation
import { TypeBuilder } from '../native';

describe('TypeBuilder', () => {
  // test that we can create classes with properties and add metadata like aliases and descriptions
  it('should provide string representation for classes with properties and metadata', () => {
    // create a fresh type builder instance to work with
    const builder = new TypeBuilder();

    // get a reference to a class named 'user', creating it if needed
    const userClass = builder.getClass('User');

    // add properties to the user class with helpful metadata
    // the name property has both an alias and description
    userClass.property('name')
      .alias('username')                     // allows referencing the property as 'username'
      .description('the user\'s full name'); // explains what this property represents

    // age property just has a description
    userClass.property('age')
      .description('user\'s age in years');  // clarifies the age units

    // email is a basic property with no extra metadata
    userClass.property('email');             // simple email field

    // convert all the type definitions to a readable string
    const output = builder.toString();

    // make sure the output has the expected class structure
    expect(output).toContain('TypeBuilder(\n  Classes: [\n    User {');
    // verify each property appears with its metadata
    expect(output).toContain("name (unknown-type) (alias=String(\"username\"), description=String(\"the user's full name\"))");
    expect(output).toContain("age (unknown-type) (description=String(\"user's age in years\"))");
    expect(output).toContain('email (unknown-type)');
  });

  // test that we can create enums with values and add metadata like aliases and descriptions
  it('should provide string representation for enums with values and metadata', () => {
    // create a fresh type builder instance to work with
    const builder = new TypeBuilder();

    // get a reference to an enum named 'status', creating it if needed
    const statusEnum = builder.getEnum('Status');

    // add possible values to the status enum with helpful metadata
    // active state has both an alias and description
    statusEnum.value('ACTIVE')
      .alias('active')                    // allows using lowercase 'active'
      .description('user is active');     // explains what active means

    // inactive state just has an alias
    statusEnum.value('INACTIVE')
      .alias('inactive');                 // allows using lowercase 'inactive'

    // pending is a basic value with no extra metadata
    statusEnum.value('PENDING');          // simple pending state

    // convert all the type definitions to a readable string
    const output = builder.toString();

    // make sure the output has the expected enum structure
    console.log(output);
    expect(output).toContain(`TypeBuilder(
  Enums: [
    Status {`);
    // verify each value appears with its metadata
    expect(output).toContain('ACTIVE (alias=String(\"active\"), description=String(\"user is active\"))');
    expect(output).toContain('INACTIVE (alias=String(\"inactive\"))');
    expect(output).toContain('PENDING');
  });
});