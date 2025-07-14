package main

import (
	"context"
	b "nested_structures/baml_client"
	"testing"
)

func TestSimpleNested(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestSimpleNested(ctx, "test simple nested")
	if err != nil {
		t.Fatalf("Error testing simple nested: %v", err)
	}

	// Verify simple nested structure
	if result.User.Id <= 0 {
		t.Errorf("Expected user.id to be positive, got %d", result.User.Id)
	}
	if result.User.Name == "" {
		t.Errorf("Expected user.name to be non-empty")
	}
	if result.User.Profile.Bio == "" {
		t.Errorf("Expected user.profile.bio to be non-empty")
	}
	if result.User.Profile.Avatar == "" {
		t.Errorf("Expected user.profile.avatar to be non-empty")
	}
	if !result.User.Profile.Preferences.Theme.IsKlight() && !result.User.Profile.Preferences.Theme.IsKdark() {
		t.Errorf("Expected user.profile.preferences.theme to be 'light' or 'dark'")
	}
	if result.User.Profile.Preferences.Language == "" {
		t.Errorf("Expected user.profile.preferences.language to be non-empty")
	}
	if !result.User.Profile.Preferences.Notifications.Frequency.IsKimmediate() && !result.User.Profile.Preferences.Notifications.Frequency.IsKdaily() && !result.User.Profile.Preferences.Notifications.Frequency.IsKweekly() {
		t.Errorf("Expected notifications.frequency to be valid")
	}
	if !result.User.Settings.Privacy.ProfileVisibility.IsKpublic() && !result.User.Settings.Privacy.ProfileVisibility.IsKprivate() && !result.User.Settings.Privacy.ProfileVisibility.IsKfriends() {
		t.Errorf("Expected privacy.profileVisibility to be valid")
	}
	if result.User.Settings.Display.FontSize <= 0 {
		t.Errorf("Expected display.fontSize to be positive, got %d", result.User.Settings.Display.FontSize)
	}
	if result.User.Settings.Display.ColorScheme == "" {
		t.Errorf("Expected display.colorScheme to be non-empty")
	}
	if len(result.User.Settings.Advanced) < 0 {
		t.Errorf("Expected advanced settings to be a valid map")
	}

	// Verify address
	if result.Address.Street == "" {
		t.Errorf("Expected address.street to be non-empty")
	}
	if result.Address.City == "" {
		t.Errorf("Expected address.city to be non-empty")
	}
	if result.Address.State == "" {
		t.Errorf("Expected address.state to be non-empty")
	}
	if result.Address.Country == "" {
		t.Errorf("Expected address.country to be non-empty")
	}
	if result.Address.PostalCode == "" {
		t.Errorf("Expected address.postalCode to be non-empty")
	}

	// Verify metadata
	if result.Metadata.CreatedAt == "" {
		t.Errorf("Expected metadata.createdAt to be non-empty")
	}
	if result.Metadata.UpdatedAt == "" {
		t.Errorf("Expected metadata.updatedAt to be non-empty")
	}
	if result.Metadata.Version <= 0 {
		t.Errorf("Expected metadata.version to be positive, got %d", result.Metadata.Version)
	}
	if len(result.Metadata.Tags) < 0 {
		t.Errorf("Expected metadata.tags to be a valid array")
	}
	if len(result.Metadata.Attributes) < 0 {
		t.Errorf("Expected metadata.attributes to be a valid map")
	}
}

func TestDeeplyNested(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestDeeplyNested(ctx, "test deeply nested")
	if err != nil {
		t.Fatalf("Error testing deeply nested: %v", err)
	}

	// Verify deeply nested structure (5 levels deep)
	if result.Level1.Data == "" {
		t.Errorf("Expected level1.data to be non-empty")
	}
	if result.Level1.Level2.Data == "" {
		t.Errorf("Expected level1.level2.data to be non-empty")
	}
	if result.Level1.Level2.Level3.Data == "" {
		t.Errorf("Expected level1.level2.level3.data to be non-empty")
	}
	if result.Level1.Level2.Level3.Level4.Data == "" {
		t.Errorf("Expected level1.level2.level3.level4.data to be non-empty")
	}
	if result.Level1.Level2.Level3.Level4.Level5.Data == "" {
		t.Errorf("Expected level1.level2.level3.level4.level5.data to be non-empty")
	}
	if len(result.Level1.Level2.Level3.Level4.Level5.Items) < 0 {
		t.Errorf("Expected level5.items to be a valid array")
	}
	if len(result.Level1.Level2.Level3.Level4.Level5.Mapping) < 0 {
		t.Errorf("Expected level5.mapping to be a valid map")
	}
}

func TestComplexNested(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestComplexNested(ctx, "test complex nested")
	if err != nil {
		t.Fatalf("Error testing complex nested: %v", err)
	}

	// Verify complex nested structure
	if result.Company.Id <= 0 {
		t.Errorf("Expected company.id to be positive, got %d", result.Company.Id)
	}
	if result.Company.Name == "" {
		t.Errorf("Expected company.name to be non-empty")
	}
	if len(result.Company.Departments) != 2 {
		t.Errorf("Expected 2 departments, got %d", len(result.Company.Departments))
	}
	if result.Company.Metadata.Founded == "" {
		t.Errorf("Expected company.metadata.founded to be non-empty")
	}
	if result.Company.Metadata.Industry == "" {
		t.Errorf("Expected company.metadata.industry to be non-empty")
	}
	if !result.Company.Metadata.Size.IsKsmall() && !result.Company.Metadata.Size.IsKmedium() && !result.Company.Metadata.Size.IsKlarge() && !result.Company.Metadata.Size.IsKenterprise() {
		t.Errorf("Expected company.metadata.size to be valid")
	}

	// Verify departments
	for i, dept := range result.Company.Departments {
		if dept.Id <= 0 {
			t.Errorf("Department %d has invalid id: %d", i, dept.Id)
		}
		if dept.Name == "" {
			t.Errorf("Department %d has empty name", i)
		}
		if dept.Budget <= 0 {
			t.Errorf("Department %d has invalid budget: %f", i, dept.Budget)
		}
	}

	// Verify employees
	if len(result.Employees) != 5 {
		t.Errorf("Expected 5 employees, got %d", len(result.Employees))
	}
	for i, emp := range result.Employees {
		if emp.Id <= 0 {
			t.Errorf("Employee %d has invalid id: %d", i, emp.Id)
		}
		if emp.Name == "" {
			t.Errorf("Employee %d has empty name", i)
		}
		if emp.Email == "" {
			t.Errorf("Employee %d has empty email", i)
		}
		if emp.Role == "" {
			t.Errorf("Employee %d has empty role", i)
		}
		if emp.Department == "" {
			t.Errorf("Employee %d has empty department", i)
		}
	}

	// Verify projects
	if len(result.Projects) != 2 {
		t.Errorf("Expected 2 projects, got %d", len(result.Projects))
	}
	for i, proj := range result.Projects {
		if proj.Id <= 0 {
			t.Errorf("Project %d has invalid id: %d", i, proj.Id)
		}
		if proj.Name == "" {
			t.Errorf("Project %d has empty name", i)
		}
		if proj.Description == "" {
			t.Errorf("Project %d has empty description", i)
		}
		if !proj.Status.IsKplanning() && !proj.Status.IsKactive() && !proj.Status.IsKcompleted() && !proj.Status.IsKcancelled() {
			t.Errorf("Project %d has invalid status", i)
		}
		if proj.Budget.Total <= 0 {
			t.Errorf("Project %d has invalid budget total: %f", i, proj.Budget.Total)
		}
		if proj.Budget.Spent < 0 {
			t.Errorf("Project %d has invalid budget spent: %f", i, proj.Budget.Spent)
		}
		if len(proj.Budget.Categories) < 0 {
			t.Errorf("Project %d has invalid budget categories", i)
		}
	}
}

func TestRecursiveStructure(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	result, err := b.TestRecursiveStructure(ctx, "test recursive structure")
	if err != nil {
		t.Fatalf("Error testing recursive structure: %v", err)
	}

	// Verify recursive structure
	if result.Id <= 0 {
		t.Errorf("Expected root.id to be positive, got %d", result.Id)
	}
	if result.Name == "" {
		t.Errorf("Expected root.name to be non-empty")
	}
	if len(result.Children) < 2 {
		t.Errorf("Expected at least 2 children, got %d", len(result.Children))
	}

	// Verify children have proper structure
	for i, child := range result.Children {
		if child.Id <= 0 {
			t.Errorf("Child %d has invalid id: %d", i, child.Id)
		}
		if child.Name == "" {
			t.Errorf("Child %d has empty name", i)
		}
		if child.Parent != nil {
			if child.Parent.Id != result.Id {
				t.Errorf("Child %d parent id mismatch: expected %d, got %d", i, result.Id, child.Parent.Id)
			}
		}
	}

	// Check for 3-level depth
	hasGrandChildren := false
	for _, child := range result.Children {
		if len(child.Children) > 0 {
			hasGrandChildren = true
			break
		}
	}
	if !hasGrandChildren {
		t.Errorf("Expected at least one child to have children (3 levels deep)")
	}
}
