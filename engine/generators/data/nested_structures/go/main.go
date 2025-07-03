package main

import (
	"context"
	"fmt"
	b "nested_structures/baml_client"
	"os"
)

func main() {
	ctx := context.Background()

	// Test simple nested structures
	fmt.Println("Testing SimpleNested...")
	simpleResult, err := b.TestSimpleNested(ctx, "test simple nested")
	if err != nil {
		fmt.Printf("Error testing simple nested: %v\n", err)
		os.Exit(1)
	}

	// Verify simple nested structure
	if simpleResult.User.Id <= 0 {
		fmt.Printf("Expected user.id to be positive, got %d\n", simpleResult.User.Id)
		os.Exit(1)
	}
	if simpleResult.User.Name == "" {
		fmt.Printf("Expected user.name to be non-empty\n")
		os.Exit(1)
	}
	if simpleResult.User.Profile.Bio == "" {
		fmt.Printf("Expected user.profile.bio to be non-empty\n")
		os.Exit(1)
	}
	if simpleResult.User.Profile.Avatar == "" {
		fmt.Printf("Expected user.profile.avatar to be non-empty\n")
		os.Exit(1)
	}
	if !simpleResult.User.Profile.Preferences.Theme.IsKlight() && !simpleResult.User.Profile.Preferences.Theme.IsKdark() {
		fmt.Printf("Expected user.profile.preferences.theme to be 'light' or 'dark'\n")
		os.Exit(1)
	}
	if simpleResult.User.Profile.Preferences.Language == "" {
		fmt.Printf("Expected user.profile.preferences.language to be non-empty\n")
		os.Exit(1)
	}
	if !simpleResult.User.Profile.Preferences.Notifications.Frequency.IsKimmediate() && !simpleResult.User.Profile.Preferences.Notifications.Frequency.IsKdaily() && !simpleResult.User.Profile.Preferences.Notifications.Frequency.IsKweekly() {
		fmt.Printf("Expected notifications.frequency to be valid\n")
		os.Exit(1)
	}
	if !simpleResult.User.Settings.Privacy.ProfileVisibility.IsKpublic() && !simpleResult.User.Settings.Privacy.ProfileVisibility.IsKprivate() && !simpleResult.User.Settings.Privacy.ProfileVisibility.IsKfriends() {
		fmt.Printf("Expected privacy.profileVisibility to be valid\n")
		os.Exit(1)
	}
	if simpleResult.User.Settings.Display.FontSize <= 0 {
		fmt.Printf("Expected display.fontSize to be positive, got %d\n", simpleResult.User.Settings.Display.FontSize)
		os.Exit(1)
	}
	if simpleResult.User.Settings.Display.ColorScheme == "" {
		fmt.Printf("Expected display.colorScheme to be non-empty\n")
		os.Exit(1)
	}
	if len(simpleResult.User.Settings.Advanced) < 0 {
		fmt.Printf("Expected advanced settings to be a valid map\n")
		os.Exit(1)
	}

	// Verify address
	if simpleResult.Address.Street == "" {
		fmt.Printf("Expected address.street to be non-empty\n")
		os.Exit(1)
	}
	if simpleResult.Address.City == "" {
		fmt.Printf("Expected address.city to be non-empty\n")
		os.Exit(1)
	}
	if simpleResult.Address.State == "" {
		fmt.Printf("Expected address.state to be non-empty\n")
		os.Exit(1)
	}
	if simpleResult.Address.Country == "" {
		fmt.Printf("Expected address.country to be non-empty\n")
		os.Exit(1)
	}
	if simpleResult.Address.PostalCode == "" {
		fmt.Printf("Expected address.postalCode to be non-empty\n")
		os.Exit(1)
	}

	// Verify metadata
	if simpleResult.Metadata.CreatedAt == "" {
		fmt.Printf("Expected metadata.createdAt to be non-empty\n")
		os.Exit(1)
	}
	if simpleResult.Metadata.UpdatedAt == "" {
		fmt.Printf("Expected metadata.updatedAt to be non-empty\n")
		os.Exit(1)
	}
	if simpleResult.Metadata.Version <= 0 {
		fmt.Printf("Expected metadata.version to be positive, got %d\n", simpleResult.Metadata.Version)
		os.Exit(1)
	}
	if len(simpleResult.Metadata.Tags) < 0 {
		fmt.Printf("Expected metadata.tags to be a valid array\n")
		os.Exit(1)
	}
	if len(simpleResult.Metadata.Attributes) < 0 {
		fmt.Printf("Expected metadata.attributes to be a valid map\n")
		os.Exit(1)
	}
	fmt.Println("✓ SimpleNested test passed")

	// Test deeply nested structures
	fmt.Println("\nTesting DeeplyNested...")
	deepResult, err := b.TestDeeplyNested(ctx, "test deeply nested")
	if err != nil {
		fmt.Printf("Error testing deeply nested: %v\n", err)
		os.Exit(1)
	}

	// Verify deeply nested structure (5 levels deep)
	if deepResult.Level1.Data == "" {
		fmt.Printf("Expected level1.data to be non-empty\n")
		os.Exit(1)
	}
	if deepResult.Level1.Level2.Data == "" {
		fmt.Printf("Expected level1.level2.data to be non-empty\n")
		os.Exit(1)
	}
	if deepResult.Level1.Level2.Level3.Data == "" {
		fmt.Printf("Expected level1.level2.level3.data to be non-empty\n")
		os.Exit(1)
	}
	if deepResult.Level1.Level2.Level3.Level4.Data == "" {
		fmt.Printf("Expected level1.level2.level3.level4.data to be non-empty\n")
		os.Exit(1)
	}
	if deepResult.Level1.Level2.Level3.Level4.Level5.Data == "" {
		fmt.Printf("Expected level1.level2.level3.level4.level5.data to be non-empty\n")
		os.Exit(1)
	}
	if len(deepResult.Level1.Level2.Level3.Level4.Level5.Items) < 0 {
		fmt.Printf("Expected level5.items to be a valid array\n")
		os.Exit(1)
	}
	if len(deepResult.Level1.Level2.Level3.Level4.Level5.Mapping) < 0 {
		fmt.Printf("Expected level5.mapping to be a valid map\n")
		os.Exit(1)
	}
	fmt.Println("✓ DeeplyNested test passed")

	// Test complex nested structures
	fmt.Println("\nTesting ComplexNested...")
	complexResult, err := b.TestComplexNested(ctx, "test complex nested")
	if err != nil {
		fmt.Printf("Error testing complex nested: %v\n", err)
		os.Exit(1)
	}

	// Verify complex nested structure
	if complexResult.Company.Id <= 0 {
		fmt.Printf("Expected company.id to be positive, got %d\n", complexResult.Company.Id)
		os.Exit(1)
	}
	if complexResult.Company.Name == "" {
		fmt.Printf("Expected company.name to be non-empty\n")
		os.Exit(1)
	}
	if len(complexResult.Company.Departments) != 2 {
		fmt.Printf("Expected 2 departments, got %d\n", len(complexResult.Company.Departments))
		os.Exit(1)
	}
	if complexResult.Company.Metadata.Founded == "" {
		fmt.Printf("Expected company.metadata.founded to be non-empty\n")
		os.Exit(1)
	}
	if complexResult.Company.Metadata.Industry == "" {
		fmt.Printf("Expected company.metadata.industry to be non-empty\n")
		os.Exit(1)
	}
	if !complexResult.Company.Metadata.Size.IsKsmall() && !complexResult.Company.Metadata.Size.IsKmedium() && !complexResult.Company.Metadata.Size.IsKlarge() && !complexResult.Company.Metadata.Size.IsKenterprise() {
		fmt.Printf("Expected company.metadata.size to be valid\n")
		os.Exit(1)
	}

	// Verify departments
	for i, dept := range complexResult.Company.Departments {
		if dept.Id <= 0 {
			fmt.Printf("Department %d has invalid id: %d\n", i, dept.Id)
			os.Exit(1)
		}
		if dept.Name == "" {
			fmt.Printf("Department %d has empty name\n", i)
			os.Exit(1)
		}
		if dept.Budget <= 0 {
			fmt.Printf("Department %d has invalid budget: %f\n", i, dept.Budget)
			os.Exit(1)
		}
	}

	// Verify employees
	if len(complexResult.Employees) != 5 {
		fmt.Printf("Expected 5 employees, got %d\n", len(complexResult.Employees))
		os.Exit(1)
	}
	for i, emp := range complexResult.Employees {
		if emp.Id <= 0 {
			fmt.Printf("Employee %d has invalid id: %d\n", i, emp.Id)
			os.Exit(1)
		}
		if emp.Name == "" {
			fmt.Printf("Employee %d has empty name\n", i)
			os.Exit(1)
		}
		if emp.Email == "" {
			fmt.Printf("Employee %d has empty email\n", i)
			os.Exit(1)
		}
		if emp.Role == "" {
			fmt.Printf("Employee %d has empty role\n", i)
			os.Exit(1)
		}
		if emp.Department == "" {
			fmt.Printf("Employee %d has empty department\n", i)
			os.Exit(1)
		}
	}

	// Verify projects
	if len(complexResult.Projects) != 2 {
		fmt.Printf("Expected 2 projects, got %d\n", len(complexResult.Projects))
		os.Exit(1)
	}
	for i, proj := range complexResult.Projects {
		if proj.Id <= 0 {
			fmt.Printf("Project %d has invalid id: %d\n", i, proj.Id)
			os.Exit(1)
		}
		if proj.Name == "" {
			fmt.Printf("Project %d has empty name\n", i)
			os.Exit(1)
		}
		if proj.Description == "" {
			fmt.Printf("Project %d has empty description\n", i)
			os.Exit(1)
		}
		if !proj.Status.IsKplanning() && !proj.Status.IsKactive() && !proj.Status.IsKcompleted() && !proj.Status.IsKcancelled() {
			fmt.Printf("Project %d has invalid status\n", i)
			os.Exit(1)
		}
		if proj.Budget.Total <= 0 {
			fmt.Printf("Project %d has invalid budget total: %f\n", i, proj.Budget.Total)
			os.Exit(1)
		}
		if proj.Budget.Spent < 0 {
			fmt.Printf("Project %d has invalid budget spent: %f\n", i, proj.Budget.Spent)
			os.Exit(1)
		}
		if len(proj.Budget.Categories) < 0 {
			fmt.Printf("Project %d has invalid budget categories\n", i)
			os.Exit(1)
		}
	}
	fmt.Println("✓ ComplexNested test passed")

	// Test recursive structures
	fmt.Println("\nTesting RecursiveStructure...")
	recursiveResult, err := b.TestRecursiveStructure(ctx, "test recursive structure")
	if err != nil {
		fmt.Printf("Error testing recursive structure: %v\n", err)
		os.Exit(1)
	}

	// Verify recursive structure
	if recursiveResult.Id <= 0 {
		fmt.Printf("Expected root.id to be positive, got %d\n", recursiveResult.Id)
		os.Exit(1)
	}
	if recursiveResult.Name == "" {
		fmt.Printf("Expected root.name to be non-empty\n")
		os.Exit(1)
	}
	if len(recursiveResult.Children) < 2 {
		fmt.Printf("Expected at least 2 children, got %d\n", len(recursiveResult.Children))
		os.Exit(1)
	}

	// Verify children have proper structure
	for i, child := range recursiveResult.Children {
		if child.Id <= 0 {
			fmt.Printf("Child %d has invalid id: %d\n", i, child.Id)
			os.Exit(1)
		}
		if child.Name == "" {
			fmt.Printf("Child %d has empty name\n", i)
			os.Exit(1)
		}
		if child.Parent != nil {
			if child.Parent.Id != recursiveResult.Id {
				fmt.Printf("Child %d parent id mismatch: expected %d, got %d\n", i, recursiveResult.Id, child.Parent.Id)
				os.Exit(1)
			}
		}
	}

	// Check for 3-level depth
	hasGrandChildren := false
	for _, child := range recursiveResult.Children {
		if len(child.Children) > 0 {
			hasGrandChildren = true
			break
		}
	}
	if !hasGrandChildren {
		fmt.Printf("Expected at least one child to have children (3 levels deep)\n")
		os.Exit(1)
	}
	fmt.Println("✓ RecursiveStructure test passed")

	fmt.Println("\n✅ All nested structure tests passed!")
}
