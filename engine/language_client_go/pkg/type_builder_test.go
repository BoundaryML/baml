package baml

import (
	"fmt"
	"strings"
	"testing"
)

// Helper function for string contains check
func contains(s, substr string) bool {
	return strings.Contains(s, substr)
}

func TestTypeBuilderBasicTypes(t *testing.T) {
	rt, err := CreateRuntime(".", map[string]string{}, map[string]string{})
	if err != nil {
		t.Fatalf("Failed to create runtime: %v", err)
	}

	// Test creating a type builder
	typeBuilder, err := rt.NewTypeBuilder()
	if err != nil {
		t.Fatalf("Failed to create type builder: %v", err)
	}

	// Test basic types
	stringType, err := typeBuilder.String()
	if err != nil {
		t.Errorf("Failed to create string type: %v", err)
	} else {
		typeStr := stringType.Print()
		if typeStr != "string" {
			t.Errorf("Expected string type to be 'string', got: %s", typeStr)
		} else {
			t.Logf("Successfully created string type: %s", typeStr)
		}
	}

	intType, err := typeBuilder.Int()
	if err != nil {
		t.Errorf("Failed to create int type: %v", err)
	} else {
		typeStr := intType.Print()
		if typeStr != "int" {
			t.Errorf("Expected int type to be 'int', got: %s", typeStr)
		} else {
			t.Logf("Successfully created int type: %s", typeStr)
		}
	}

	floatType, err := typeBuilder.Float()
	if err != nil {
		t.Errorf("Failed to create float type: %v", err)
	} else {
		typeStr := floatType.Print()
		if typeStr != "float" {
			t.Errorf("Expected float type to be 'float', got: %s", typeStr)
		} else {
			t.Logf("Successfully created float type: %s", typeStr)
		}
	}

	boolType, err := typeBuilder.Bool()
	if err != nil {
		t.Errorf("Failed to create bool type: %v", err)
	} else {
		typeStr := boolType.Print()
		if typeStr != "bool" {
			t.Errorf("Expected bool type to be 'bool', got: %s", typeStr)
		} else {
			t.Logf("Successfully created bool type: %s", typeStr)
		}
	}

	nullType, err := typeBuilder.Null()
	if err != nil {
		t.Errorf("Failed to create null type: %v", err)
	} else {
		typeStr := nullType.Print()
		if typeStr != "null" {
			t.Errorf("Expected null type to be 'null', got: %s", typeStr)
		} else {
			t.Logf("Successfully created null type: %s", typeStr)
		}
	}

	// Test type extensions
	if stringType != nil {
		listType, err := stringType.List()
		if err != nil {
			t.Errorf("Failed to create list type: %v", err)
		} else {
			typeStr := listType.Print()
			t.Logf("Successfully created list of string type: %s", typeStr)
		}

		optionalType, err := stringType.Optional()
		if err != nil {
			t.Errorf("Failed to create optional type: %v", err)
		} else {
			typeStr := optionalType.Print()
			t.Logf("Successfully created optional string type: %s", typeStr)
		}
	}

	t.Log("Basic types test completed successfully")
}

func TestTypeBuilderLiteralTypes(t *testing.T) {
	rt, err := CreateRuntime(".", map[string]string{}, map[string]string{})
	if err != nil {
		t.Fatalf("Failed to create runtime: %v", err)
	}

	typeBuilder, err := rt.NewTypeBuilder()
	if err != nil {
		t.Fatalf("Failed to create type builder: %v", err)
	}

	// Test literal types
	literalString, err := typeBuilder.LiteralString("hello")
	if err != nil {
		t.Errorf("Failed to create literal string: %v", err)
	} else {
		typeStr := literalString.Print()
		t.Logf("Successfully created literal string type: %s", typeStr)
	}

	literalInt, err := typeBuilder.LiteralInt(42)
	if err != nil {
		t.Errorf("Failed to create literal int: %v", err)
	} else {
		typeStr := literalInt.Print()
		t.Logf("Successfully created literal int type: %s", typeStr)
	}

	literalBool, err := typeBuilder.LiteralBool(true)
	if err != nil {
		t.Errorf("Failed to create literal bool: %v", err)
	} else {
		typeStr := literalBool.Print()
		t.Logf("Successfully created literal bool type: %s", typeStr)
	}

	t.Log("Literal types test completed successfully")
}

func TestTypeBuilderCompositeTypes(t *testing.T) {
	rt, err := CreateRuntime(".", map[string]string{}, map[string]string{})
	if err != nil {
		t.Fatalf("Failed to create runtime: %v", err)
	}

	typeBuilder, err := rt.NewTypeBuilder()
	if err != nil {
		t.Fatalf("Failed to create type builder: %v", err)
	}

	// Create basic types for composition
	stringType, err := typeBuilder.String()
	if err != nil {
		t.Fatalf("Failed to create string type: %v", err)
	}

	intType, err := typeBuilder.Int()
	if err != nil {
		t.Fatalf("Failed to create int type: %v", err)
	}

	// Test composite types
	mapType, err := typeBuilder.Map(stringType, intType)
	if err != nil {
		t.Errorf("Failed to create map type: %v", err)
	} else {
		typeStr := mapType.Print()
		t.Logf("Successfully created map type: %s", typeStr)
	}

	listType, err := typeBuilder.List(stringType)
	if err != nil {
		t.Errorf("Failed to create list type: %v", err)
	} else {
		typeStr := listType.Print()
		t.Logf("Successfully created list type: %s", typeStr)
	}

	optionalType, err := typeBuilder.Optional(intType)
	if err != nil {
		t.Errorf("Failed to create optional type: %v", err)
	} else {
		typeStr := optionalType.Print()
		t.Logf("Successfully created optional type: %s", typeStr)
	}

	// Test union type
	unionTypes := []Type{stringType, intType}
	unionType, err := typeBuilder.Union(unionTypes)
	if err != nil {
		t.Errorf("Failed to create union type: %v", err)
	} else {
		typeStr := unionType.Print()
		t.Logf("Successfully created union type: %s", typeStr)
	}

	t.Log("Composite types test completed successfully")
}

func TestTypeBuilderEnums(t *testing.T) {
	rt, err := CreateRuntime(".", map[string]string{}, map[string]string{})
	if err != nil {
		t.Fatalf("Failed to create runtime: %v", err)
	}

	typeBuilder, err := rt.NewTypeBuilder()
	if err != nil {
		t.Fatalf("Failed to create type builder: %v", err)
	}

	// Test creating multiple enums for comprehensive testing
	statusEnum, err := typeBuilder.AddEnum("Status")
	if err != nil {
		t.Errorf("Failed to create Status enum: %v", err)
		return
	}
	t.Log("Successfully created Status enum")

	priorityEnum, err := typeBuilder.AddEnum("Priority")
	if err != nil {
		t.Errorf("Failed to create Priority enum: %v", err)
		return
	}
	t.Log("Successfully created Priority enum")

	categoryEnum, err := typeBuilder.AddEnum("Category")
	if err != nil {
		t.Errorf("Failed to create Category enum: %v", err)
		return
	}
	t.Log("Successfully created Category enum")

	// Test enum descriptions and aliases
	err = statusEnum.SetDescription("Status enumeration for tasks")
	if err != nil {
		t.Errorf("Failed to set Status enum description: %v", err)
	}

	// Validate the description was set correctly
	statusDesc, err := statusEnum.Description()
	if err != nil {
		t.Errorf("Failed to get Status enum description: %v", err)
	} else if statusDesc == nil {
		t.Errorf("Expected Status enum description to be set, but got nil")
	} else if *statusDesc != "Status enumeration for tasks" {
		t.Errorf("Expected Status enum description to be 'Status enumeration for tasks', got: %s", *statusDesc)
	} else {
		t.Logf("Successfully validated Status enum description: %s", *statusDesc)
	}

	err = statusEnum.SetAlias("task_status")
	if err != nil {
		t.Errorf("Failed to set Status enum alias: %v", err)
	}

	// Validate the alias was set correctly
	statusAlias, err := statusEnum.Alias()
	if err != nil {
		t.Errorf("Failed to get Status enum alias: %v", err)
	} else if statusAlias == nil {
		t.Errorf("Expected Status enum alias to be set, but got nil")
	} else if *statusAlias != "task_status" {
		t.Errorf("Expected Status enum alias to be 'task_status', got: %s", *statusAlias)
	} else {
		t.Logf("Successfully validated Status enum alias: %s", *statusAlias)
	}

	// Add values to Status enum
	activeValue, err := statusEnum.AddValue("ACTIVE")
	if err != nil {
		t.Errorf("Failed to add ACTIVE value: %v", err)
	} else {
		err = activeValue.SetDescription("Task is currently active")
		if err != nil {
			t.Errorf("Failed to set ACTIVE value description: %v", err)
		}

		// Validate the ACTIVE value description
		activeDesc, err := activeValue.Description()
		if err != nil {
			t.Errorf("Failed to get ACTIVE value description: %v", err)
		} else if activeDesc == nil {
			t.Errorf("Expected ACTIVE value description to be set, but got nil")
		} else if *activeDesc != "Task is currently active" {
			t.Errorf("Expected ACTIVE value description to be 'Task is currently active', got: %s", *activeDesc)
		} else {
			t.Logf("Successfully validated ACTIVE value description: %s", *activeDesc)
		}

		err = activeValue.SetAlias("active_state")
		if err != nil {
			t.Errorf("Failed to set ACTIVE value alias: %v", err)
		}

		// Validate the ACTIVE value alias
		activeAlias, err := activeValue.Alias()
		if err != nil {
			t.Errorf("Failed to get ACTIVE value alias: %v", err)
		} else if activeAlias == nil {
			t.Errorf("Expected ACTIVE value alias to be set, but got nil")
		} else if *activeAlias != "active_state" {
			t.Errorf("Expected ACTIVE value alias to be 'active_state', got: %s", *activeAlias)
		} else {
			t.Logf("Successfully validated ACTIVE value alias: %s", *activeAlias)
		}

		t.Log("Successfully added and configured ACTIVE value")
	}

	inactiveValue, err := statusEnum.AddValue("INACTIVE")
	if err != nil {
		t.Errorf("Failed to add INACTIVE value: %v", err)
	} else {
		err = inactiveValue.SetDescription("Task is inactive")
		if err != nil {
			t.Errorf("Failed to set INACTIVE value description: %v", err)
		}

		// Validate the INACTIVE value description
		inactiveDesc, err := inactiveValue.Description()
		if err != nil {
			t.Errorf("Failed to get INACTIVE value description: %v", err)
		} else if inactiveDesc == nil {
			t.Errorf("Expected INACTIVE value description to be set, but got nil")
		} else if *inactiveDesc != "Task is inactive" {
			t.Errorf("Expected INACTIVE value description to be 'Task is inactive', got: %s", *inactiveDesc)
		} else {
			t.Logf("Successfully validated INACTIVE value description: %s", *inactiveDesc)
		}

		t.Log("Successfully added INACTIVE value")
	}

	_, err = statusEnum.AddValue("PENDING")
	if err != nil {
		t.Errorf("Failed to add PENDING value: %v", err)
	} else {
		t.Log("Successfully added PENDING value")
	}

	_, err = statusEnum.AddValue("COMPLETED")
	if err != nil {
		t.Errorf("Failed to add COMPLETED value: %v", err)
	} else {
		t.Log("Successfully added COMPLETED value")
	}

	// Add values to Priority enum
	_, err = priorityEnum.AddValue("HIGH")
	if err != nil {
		t.Errorf("Failed to add HIGH value: %v", err)
	} else {
		t.Log("Successfully added HIGH value")
	}

	_, err = priorityEnum.AddValue("MEDIUM")
	if err != nil {
		t.Errorf("Failed to add MEDIUM value: %v", err)
	} else {
		t.Log("Successfully added MEDIUM value")
	}

	_, err = priorityEnum.AddValue("LOW")
	if err != nil {
		t.Errorf("Failed to add LOW value: %v", err)
	} else {
		t.Log("Successfully added LOW value")
	}

	// Add values to Category enum
	_, err = categoryEnum.AddValue("WORK")
	if err != nil {
		t.Errorf("Failed to add WORK value: %v", err)
	}

	_, err = categoryEnum.AddValue("PERSONAL")
	if err != nil {
		t.Errorf("Failed to add PERSONAL value: %v", err)
	}

	// Test listing values for each enum and verify counts
	statusValues, err := statusEnum.ListValues()
	if err != nil {
		t.Errorf("Failed to list Status enum values: %v", err)
	} else {
		t.Logf("Status enum has %d values", len(statusValues))
		if len(statusValues) != 4 {
			t.Errorf("Expected 4 Status values, got %d", len(statusValues))
		}
	}

	priorityValues, err := priorityEnum.ListValues()
	if err != nil {
		t.Errorf("Failed to list Priority enum values: %v", err)
	} else {
		t.Logf("Priority enum has %d values", len(priorityValues))
		if len(priorityValues) != 3 {
			t.Errorf("Expected 3 Priority values, got %d", len(priorityValues))
		}
	}

	categoryValues, err := categoryEnum.ListValues()
	if err != nil {
		t.Errorf("Failed to list Category enum values: %v", err)
	} else {
		t.Logf("Category enum has %d values", len(categoryValues))
		if len(categoryValues) != 2 {
			t.Errorf("Expected 2 Category values, got %d", len(categoryValues))
		}
	}

	// Test retrieving specific values by name from different enums
	_, err = statusEnum.Value("ACTIVE")
	if err != nil {
		t.Errorf("Failed to get ACTIVE value by name: %v", err)
	} else {
		t.Log("Successfully retrieved ACTIVE value by name")
	}

	_, err = priorityEnum.Value("HIGH")
	if err != nil {
		t.Errorf("Failed to get HIGH value by name: %v", err)
	} else {
		t.Log("Successfully retrieved HIGH value by name")
	}

	_, err = categoryEnum.Value("WORK")
	if err != nil {
		t.Errorf("Failed to get WORK value by name: %v", err)
	} else {
		t.Log("Successfully retrieved WORK value by name")
	}

	// Test getting enum types and their string representations
	statusType, err := statusEnum.Type()
	if err != nil {
		t.Errorf("Failed to get Status enum type: %v", err)
	} else {
		typeStr := statusType.Print()
		t.Logf("Status enum type: %s", typeStr)
	}

	priorityType, err := priorityEnum.Type()
	if err != nil {
		t.Errorf("Failed to get Priority enum type: %v", err)
	} else {
		typeStr := priorityType.Print()
		t.Logf("Priority enum type: %s", typeStr)
	}

	categoryType, err := categoryEnum.Type()
	if err != nil {
		t.Errorf("Failed to get Category enum type: %v", err)
	} else {
		typeStr := categoryType.Print()
		t.Logf("Category enum type: %s", typeStr)
	}

	// Test listing all enums and verify total count
	allEnums, err := typeBuilder.ListEnums()
	if err != nil {
		t.Errorf("Failed to list all enums: %v", err)
	} else {
		t.Logf("Total enums in system: %d", len(allEnums))
		if len(allEnums) != 3 {
			t.Errorf("Expected 3 total enums, got %d", len(allEnums))
		}
	}

	// Test retrieving existing enums by name and verify functionality
	existingStatus, err := typeBuilder.Enum("Status")
	if err != nil {
		t.Errorf("Failed to get existing Status enum: %v", err)
	} else {
		t.Log("Successfully retrieved existing Status enum")
		// Verify we can still access values from retrieved enum
		_, err := existingStatus.Value("ACTIVE")
		if err != nil {
			t.Errorf("Failed to access ACTIVE value from retrieved Status enum: %v", err)
		} else {
			t.Log("Successfully accessed ACTIVE value from retrieved Status enum")
		}

		// Verify we can list values from retrieved enum
		retrievedValues, err := existingStatus.ListValues()
		if err != nil {
			t.Errorf("Failed to list values from retrieved Status enum: %v", err)
		} else {
			t.Logf("Retrieved Status enum has %d values", len(retrievedValues))
		}
	}

	_, err = typeBuilder.Enum("Priority")
	if err != nil {
		t.Errorf("Failed to get existing Priority enum: %v", err)
	} else {
		t.Log("Successfully retrieved existing Priority enum")
	}

	// Test error cases - trying to get non-existent enum/values
	_, err = typeBuilder.Enum("NonExistentEnum")
	if err == nil {
		t.Log("Warning: Expected error when getting non-existent enum, but got none")
	} else {
		t.Logf("Correctly got error for non-existent enum: %v", err)
	}

	_, err = statusEnum.Value("NON_EXISTENT_VALUE")
	if err == nil {
		t.Log("Warning: Expected error when getting non-existent value, but got none")
	} else {
		t.Logf("Correctly got error for non-existent value: %v", err)
	}

	t.Log("Enhanced enum operations test completed successfully")
}

func TestTypeBuilderClasses(t *testing.T) {
	rt, err := CreateRuntime(".", map[string]string{}, map[string]string{})
	if err != nil {
		t.Fatalf("Failed to create runtime: %v", err)
	}

	typeBuilder, err := rt.NewTypeBuilder()
	if err != nil {
		t.Fatalf("Failed to create type builder: %v", err)
	}

	// Create multiple classes for comprehensive testing
	userClass, err := typeBuilder.AddClass("User")
	if err != nil {
		t.Errorf("Failed to add User class: %v", err)
		return
	}
	t.Log("Successfully added User class")

	taskClass, err := typeBuilder.AddClass("Task")
	if err != nil {
		t.Errorf("Failed to add Task class: %v", err)
		return
	}
	t.Log("Successfully added Task class")

	projectClass, err := typeBuilder.AddClass("Project")
	if err != nil {
		t.Errorf("Failed to add Project class: %v", err)
		return
	}
	t.Log("Successfully added Project class")

	// Set class descriptions and aliases
	err = userClass.SetDescription("User information and profile")
	if err != nil {
		t.Errorf("Failed to set User class description: %v", err)
	}

	// Validate the User class description was set correctly
	userDesc, err := userClass.Description()
	if err != nil {
		t.Errorf("Failed to get User class description: %v", err)
	} else if userDesc == nil {
		t.Errorf("Expected User class description to be set, but got nil")
	} else if *userDesc != "User information and profile" {
		t.Errorf("Expected User class description to be 'User information and profile', got: %s", *userDesc)
	} else {
		t.Logf("Successfully validated User class description: %s", *userDesc)
	}

	err = userClass.SetAlias("user_profile")
	if err != nil {
		t.Errorf("Failed to set User class alias: %v", err)
	}

	// Validate the User class alias was set correctly
	userAlias, err := userClass.Alias()
	if err != nil {
		t.Errorf("Failed to get User class alias: %v", err)
	} else if userAlias == nil {
		t.Errorf("Expected User class alias to be set, but got nil")
	} else if *userAlias != "user_profile" {
		t.Errorf("Expected User class alias to be 'user_profile', got: %s", *userAlias)
	} else {
		t.Logf("Successfully validated User class alias: %s", *userAlias)
	}

	err = taskClass.SetDescription("Individual task or todo item")
	if err != nil {
		t.Errorf("Failed to set Task class description: %v", err)
	}

	// Validate the Task class description was set correctly
	taskDesc, err := taskClass.Description()
	if err != nil {
		t.Errorf("Failed to get Task class description: %v", err)
	} else if taskDesc == nil {
		t.Errorf("Expected Task class description to be set, but got nil")
	} else if *taskDesc != "Individual task or todo item" {
		t.Errorf("Expected Task class description to be 'Individual task or todo item', got: %s", *taskDesc)
	} else {
		t.Logf("Successfully validated Task class description: %s", *taskDesc)
	}

	// Create various types for properties
	stringType, err := typeBuilder.String()
	if err != nil {
		t.Fatalf("Failed to create string type: %v", err)
	}

	intType, err := typeBuilder.Int()
	if err != nil {
		t.Fatalf("Failed to create int type: %v", err)
	}

	floatType, err := typeBuilder.Float()
	if err != nil {
		t.Fatalf("Failed to create float type: %v", err)
	}

	boolType, err := typeBuilder.Bool()
	if err != nil {
		t.Fatalf("Failed to create bool type: %v", err)
	}

	// Create optional and list types
	optionalStringType, err := typeBuilder.Optional(stringType)
	if err != nil {
		t.Fatalf("Failed to create optional string type: %v", err)
	}

	stringListType, err := typeBuilder.List(stringType)
	if err != nil {
		t.Fatalf("Failed to create string list type: %v", err)
	}

	// Add properties to User class
	nameProperty, err := userClass.AddProperty("name", stringType)
	if err != nil {
		t.Errorf("Failed to add name property to User: %v", err)
	} else {
		err = nameProperty.SetDescription("User's full name")
		if err != nil {
			t.Errorf("Failed to set name property description: %v", err)
		}

		// Validate the name property description was set correctly
		nameDesc, err := nameProperty.Description()
		if err != nil {
			t.Errorf("Failed to get name property description: %v", err)
		} else if nameDesc == nil {
			t.Errorf("Expected name property description to be set, but got nil")
		} else if *nameDesc != "User's full name" {
			t.Errorf("Expected name property description to be 'User's full name', got: %s", *nameDesc)
		} else {
			t.Logf("Successfully validated name property description: %s", *nameDesc)
		}

		err = nameProperty.SetAlias("full_name")
		if err != nil {
			t.Errorf("Failed to set name property alias: %v", err)
		}

		// Validate the name property alias was set correctly
		nameAlias, err := nameProperty.Alias()
		if err != nil {
			t.Errorf("Failed to get name property alias: %v", err)
		} else if nameAlias == nil {
			t.Errorf("Expected name property alias to be set, but got nil")
		} else if *nameAlias != "full_name" {
			t.Errorf("Expected name property alias to be 'full_name', got: %s", *nameAlias)
		} else {
			t.Logf("Successfully validated name property alias: %s", *nameAlias)
		}

		// Validate the name property type
		nameType, err := nameProperty.Type()
		if err != nil {
			t.Errorf("Failed to get name property type: %v", err)
		} else {
			nameTypeStr := nameType.Print()
			if nameTypeStr != "string" {
				t.Errorf("Expected name property type to be 'string', got: %s", nameTypeStr)
			} else {
				t.Logf("Successfully validated name property type: %s", nameTypeStr)
			}
		}

		t.Log("Successfully added and configured name property")
	}

	emailProperty, err := userClass.AddProperty("email", stringType)
	if err != nil {
		t.Errorf("Failed to add email property to User: %v", err)
	} else {
		err = emailProperty.SetDescription("User's email address")
		if err != nil {
			t.Errorf("Failed to set email property description: %v", err)
		}

		// Validate the email property description was set correctly
		emailDesc, err := emailProperty.Description()
		if err != nil {
			t.Errorf("Failed to get email property description: %v", err)
		} else if emailDesc == nil {
			t.Errorf("Expected email property description to be set, but got nil")
		} else if *emailDesc != "User's email address" {
			t.Errorf("Expected email property description to be 'User's email address', got: %s", *emailDesc)
		} else {
			t.Logf("Successfully validated email property description: %s", *emailDesc)
		}

		// Validate the email property type
		emailType, err := emailProperty.Type()
		if err != nil {
			t.Errorf("Failed to get email property type: %v", err)
		} else {
			emailTypeStr := emailType.Print()
			if emailTypeStr != "string" {
				t.Errorf("Expected email property type to be 'string', got: %s", emailTypeStr)
			} else {
				t.Logf("Successfully validated email property type: %s", emailTypeStr)
			}
		}

		t.Log("Successfully added email property")
	}

	_, err = userClass.AddProperty("age", intType)
	if err != nil {
		t.Errorf("Failed to add age property to User: %v", err)
	} else {
		t.Log("Successfully added age property")
	}

	_, err = userClass.AddProperty("is_active", boolType)
	if err != nil {
		t.Errorf("Failed to add is_active property to User: %v", err)
	} else {
		t.Log("Successfully added is_active property")
	}

	_, err = userClass.AddProperty("bio", optionalStringType)
	if err != nil {
		t.Errorf("Failed to add bio property to User: %v", err)
	} else {
		t.Log("Successfully added optional bio property")
	}

	// Add properties to Task class
	_, err = taskClass.AddProperty("title", stringType)
	if err != nil {
		t.Errorf("Failed to add title property to Task: %v", err)
	} else {
		t.Log("Successfully added title property to Task")
	}

	_, err = taskClass.AddProperty("description", optionalStringType)
	if err != nil {
		t.Errorf("Failed to add description property to Task: %v", err)
	} else {
		t.Log("Successfully added description property to Task")
	}

	_, err = taskClass.AddProperty("priority", intType)
	if err != nil {
		t.Errorf("Failed to add priority property to Task: %v", err)
	} else {
		t.Log("Successfully added priority property to Task")
	}

	_, err = taskClass.AddProperty("completed", boolType)
	if err != nil {
		t.Errorf("Failed to add completed property to Task: %v", err)
	} else {
		t.Log("Successfully added completed property to Task")
	}

	// Add properties to Project class
	_, err = projectClass.AddProperty("name", stringType)
	if err != nil {
		t.Errorf("Failed to add name property to Project: %v", err)
	}

	_, err = projectClass.AddProperty("budget", floatType)
	if err != nil {
		t.Errorf("Failed to add budget property to Project: %v", err)
	}

	_, err = projectClass.AddProperty("tags", stringListType)
	if err != nil {
		t.Errorf("Failed to add tags property to Project: %v", err)
	} else {
		t.Log("Successfully added tags list property to Project")
	}

	// Test listing properties for each class and verify counts
	userProperties, err := userClass.ListProperties()
	if err != nil {
		t.Errorf("Failed to list User class properties: %v", err)
	} else {
		t.Logf("User class has %d properties", len(userProperties))
		if len(userProperties) != 5 {
			t.Errorf("Expected 5 User properties, got %d", len(userProperties))
		}
	}

	taskProperties, err := taskClass.ListProperties()
	if err != nil {
		t.Errorf("Failed to list Task class properties: %v", err)
	} else {
		t.Logf("Task class has %d properties", len(taskProperties))
		if len(taskProperties) != 4 {
			t.Errorf("Expected 4 Task properties, got %d", len(taskProperties))
		}
	}

	projectProperties, err := projectClass.ListProperties()
	if err != nil {
		t.Errorf("Failed to list Project class properties: %v", err)
	} else {
		t.Logf("Project class has %d properties", len(projectProperties))
		if len(projectProperties) != 3 {
			t.Errorf("Expected 3 Project properties, got %d", len(projectProperties))
		}
	}

	// Test retrieving specific properties by name
	_, err = userClass.Property("name")
	if err != nil {
		t.Errorf("Failed to get name property from User class: %v", err)
	} else {
		t.Log("Successfully retrieved name property from User class")
	}

	_, err = taskClass.Property("title")
	if err != nil {
		t.Errorf("Failed to get title property from Task class: %v", err)
	} else {
		t.Log("Successfully retrieved title property from Task class")
	}

	_, err = projectClass.Property("budget")
	if err != nil {
		t.Errorf("Failed to get budget property from Project class: %v", err)
	} else {
		t.Log("Successfully retrieved budget property from Project class")
	}

	// Test modifying property types
	retrievedNameProp, err := userClass.Property("name")
	if err != nil {
		t.Errorf("Failed to retrieve name property for modification: %v", err)
	} else {
		// Change name property type from string to optional string
		err = retrievedNameProp.SetType(optionalStringType)
		if err != nil {
			t.Errorf("Failed to change name property type: %v", err)
		} else {
			// Validate the type change was successful
			updatedNameType, err := retrievedNameProp.Type()
			if err != nil {
				t.Errorf("Failed to get updated name property type: %v", err)
			} else {
				updatedNameTypeStr := updatedNameType.Print()
				t.Logf("Updated name property type: %s", updatedNameTypeStr)
				if !contains(updatedNameTypeStr, "string") && !contains(updatedNameTypeStr, "optional") {
					t.Errorf("Expected updated name property type to contain 'string' and 'optional', got: %s", updatedNameTypeStr)
				} else {
					t.Logf("Successfully validated updated name property type: %s", updatedNameTypeStr)
				}
			}
			t.Log("Successfully changed name property type to optional string")
		}
	}

	// Test getting class types and their string representations
	userType, err := userClass.Type()
	if err != nil {
		t.Errorf("Failed to get User class type: %v", err)
	} else {
		typeStr := userType.Print()
		t.Logf("User class type: %s", typeStr)
	}

	taskType, err := taskClass.Type()
	if err != nil {
		t.Errorf("Failed to get Task class type: %v", err)
	} else {
		typeStr := taskType.Print()
		t.Logf("Task class type: %s", typeStr)
	}

	projectType, err := projectClass.Type()
	if err != nil {
		t.Errorf("Failed to get Project class type: %v", err)
	} else {
		typeStr := projectType.Print()
		t.Logf("Project class type: %s", typeStr)
	}

	// Test listing all classes and verify total count
	allClasses, err := typeBuilder.ListClasses()
	if err != nil {
		t.Errorf("Failed to list all classes: %v", err)
	} else {
		t.Logf("Total classes in system: %d", len(allClasses))
		if len(allClasses) != 3 {
			t.Errorf("Expected 3 total classes, got %d", len(allClasses))
		}
	}

	// Test retrieving existing classes by name and verify functionality
	existingUser, err := typeBuilder.Class("User")
	if err != nil {
		t.Errorf("Failed to get existing User class: %v", err)
	} else {
		t.Log("Successfully retrieved existing User class")
		// Verify we can still access properties from retrieved class
		_, err := existingUser.Property("name")
		if err != nil {
			t.Errorf("Failed to access name property from retrieved User class: %v", err)
		} else {
			t.Log("Successfully accessed name property from retrieved User class")
		}

		// Verify we can list properties from retrieved class
		retrievedProperties, err := existingUser.ListProperties()
		if err != nil {
			t.Errorf("Failed to list properties from retrieved User class: %v", err)
		} else {
			t.Logf("Retrieved User class has %d properties", len(retrievedProperties))
		}
	}

	_, err = typeBuilder.Class("Task")
	if err != nil {
		t.Errorf("Failed to get existing Task class: %v", err)
	} else {
		t.Log("Successfully retrieved existing Task class")
	}

	_, err = typeBuilder.Class("Project")
	if err != nil {
		t.Errorf("Failed to get existing Project class: %v", err)
	} else {
		t.Log("Successfully retrieved existing Project class")
	}

	// Test error cases - trying to get non-existent class/properties
	_, err = typeBuilder.Class("NonExistentClass")
	if err == nil {
		t.Log("Warning: Expected error when getting non-existent class, but got none")
	} else {
		t.Logf("Correctly got error for non-existent class: %v", err)
	}

	_, err = userClass.Property("non_existent_property")
	if err == nil {
		t.Log("Warning: Expected error when getting non-existent property, but got none")
	} else {
		t.Logf("Correctly got error for non-existent property: %v", err)
	}

	t.Log("Enhanced class operations test completed successfully")
}

func TestTypeBuilderBAMLSchema(t *testing.T) {
	rt, err := CreateRuntime(".", map[string]string{}, map[string]string{})
	if err != nil {
		t.Fatalf("Failed to create runtime: %v", err)
	}

	typeBuilder, err := rt.NewTypeBuilder()
	if err != nil {
		t.Fatalf("Failed to create type builder: %v", err)
	}

	// Test adding BAML schema
	bamlSchema := `
enum Status {
  ACTIVE
  INACTIVE
}

class User {
  name string
  status Status
}
`

	err = typeBuilder.AddBaml(bamlSchema)
	if err != nil {
		t.Errorf("Failed to add BAML schema: %v", err)
	} else {
		t.Log("Successfully added BAML schema")
	}

	// After adding BAML, we should be able to access the defined types
	allEnums, err := typeBuilder.ListEnums()
	if err != nil {
		t.Errorf("Failed to list enums after BAML add: %v", err)
	} else {
		t.Logf("Found %d enums after adding BAML", len(allEnums))
	}

	allClasses, err := typeBuilder.ListClasses()
	if err != nil {
		t.Errorf("Failed to list classes after BAML add: %v", err)
	} else {
		t.Logf("Found %d classes after adding BAML", len(allClasses))
	}

	t.Log("BAML schema test completed successfully")
}

func TestTypeBuilderSkipEnumValue(t *testing.T) {
	rt, err := CreateRuntime(".", map[string]string{}, map[string]string{})
	if err != nil {
		t.Fatalf("Failed to create runtime: %v", err)
	}

	typeBuilder, err := rt.NewTypeBuilder()
	if err != nil {
		t.Fatalf("Failed to create type builder: %v", err)
	}

	// Create enum with multiple values
	enumBuilder, err := typeBuilder.AddEnum("SkipTestEnum")
	if err != nil {
		t.Fatalf("Failed to add enum: %v", err)
	}

	_, err = enumBuilder.AddValue("KEEP_VALUE")
	if err != nil {
		t.Fatalf("Failed to add enum value: %v", err)
	}

	value2, err := enumBuilder.AddValue("SKIP_VALUE")
	if err != nil {
		t.Fatalf("Failed to add second enum value: %v", err)
	}

	// Test skipping a value
	err = value2.SetSkip(true)
	if err != nil {
		t.Errorf("Failed to skip enum value: %v", err)
	} else {
		skip, err := value2.Skip()
		if err != nil {
			t.Errorf("Failed to get skip value: %v", err)
		} else if !skip {
			t.Errorf("Expected skip value to be true, got false")
		}
		t.Log("Successfully skipped enum value")
	}

	t.Log("Skip enum value test completed successfully")
}

func TestTypeStringMethod(t *testing.T) {
	rt, err := CreateRuntime(".", map[string]string{}, map[string]string{})
	if err != nil {
		t.Fatalf("Failed to create runtime: %v", err)
	}

	typeBuilder, err := rt.NewTypeBuilder()
	if err != nil {
		t.Fatalf("Failed to create type builder: %v", err)
	}

	// Test basic type String() method
	stringType, err := typeBuilder.String()
	if err != nil {
		t.Fatalf("Failed to create string type: %v", err)
	}

	// Test fmt.Stringer interface - should work with fmt.Sprint, etc.
	stringRepr := fmt.Sprintf("Type: %s", stringType)
	t.Logf("String type representation: %s", stringRepr)

	// Test with more complex types
	intType, err := typeBuilder.Int()
	if err != nil {
		t.Fatalf("Failed to create int type: %v", err)
	}

	listType, err := typeBuilder.List(stringType)
	if err != nil {
		t.Fatalf("Failed to create list type: %v", err)
	}

	optionalType, err := typeBuilder.Optional(intType)
	if err != nil {
		t.Fatalf("Failed to create optional type: %v", err)
	}

	unionType, err := typeBuilder.Union([]Type{stringType, intType})
	if err != nil {
		t.Fatalf("Failed to create union type: %v", err)
	}

	// Test printing various types using native Go formatting
	t.Logf("Basic string type: %s", stringType)
	t.Logf("Basic int type: %s", intType)
	t.Logf("List type: %s", listType)
	t.Logf("Optional type: %s", optionalType)
	t.Logf("Union type: %s", unionType)

	// Test with fmt.Print style formatting
	typesList := []Type{stringType, listType, optionalType, unionType}
	for i, typ := range typesList {
		t.Logf("Type %d: %v", i, typ)
	}

	t.Log("Type String method test completed successfully")
}
