import asyncio
import sys
from baml_client import baml

async def test_simple_nested():
    print("Testing SimpleNested...")
    result = await baml.TestSimpleNested("test simple nested")
    
    # Verify user structure
    assert hasattr(result.user, 'id'), "User should have id"
    assert hasattr(result.user, 'name'), "User should have name"
    assert hasattr(result.user, 'profile'), "User should have profile"
    assert hasattr(result.user, 'settings'), "User should have settings"
    
    # Verify profile structure
    profile = result.user.profile
    assert hasattr(profile, 'bio'), "Profile should have bio"
    assert hasattr(profile, 'avatar'), "Profile should have avatar"
    assert hasattr(profile, 'social'), "Profile should have social"
    assert hasattr(profile, 'preferences'), "Profile should have preferences"
    
    # Verify social links structure
    social = profile.social
    assert hasattr(social, 'twitter'), "Social should have twitter"
    assert hasattr(social, 'github'), "Social should have github"
    assert hasattr(social, 'linkedin'), "Social should have linkedin"
    assert hasattr(social, 'website'), "Social should have website"
    
    # Verify preferences structure
    preferences = profile.preferences
    assert hasattr(preferences, 'theme'), "Preferences should have theme"
    assert hasattr(preferences, 'language'), "Preferences should have language"
    assert hasattr(preferences, 'notifications'), "Preferences should have notifications"
    assert preferences.theme in ["light", "dark"], f"Theme should be 'light' or 'dark', got {preferences.theme}"
    
    # Verify notification settings
    notifications = preferences.notifications
    assert hasattr(notifications, 'email'), "Notifications should have email"
    assert hasattr(notifications, 'push'), "Notifications should have push"
    assert hasattr(notifications, 'sms'), "Notifications should have sms"
    assert hasattr(notifications, 'frequency'), "Notifications should have frequency"
    assert notifications.frequency in ["immediate", "daily", "weekly"], f"Frequency should be valid option, got {notifications.frequency}"
    
    # Verify user settings structure
    settings = result.user.settings
    assert hasattr(settings, 'privacy'), "Settings should have privacy"
    assert hasattr(settings, 'display'), "Settings should have display"
    assert hasattr(settings, 'advanced'), "Settings should have advanced"
    
    # Verify privacy settings
    privacy = settings.privacy
    assert hasattr(privacy, 'profileVisibility'), "Privacy should have profileVisibility"
    assert hasattr(privacy, 'showEmail'), "Privacy should have showEmail"
    assert hasattr(privacy, 'showPhone'), "Privacy should have showPhone"
    assert privacy.profileVisibility in ["public", "private", "friends"], f"Profile visibility should be valid option, got {privacy.profileVisibility}"
    
    # Verify display settings
    display = settings.display
    assert hasattr(display, 'fontSize'), "Display should have fontSize"
    assert hasattr(display, 'colorScheme'), "Display should have colorScheme"
    assert hasattr(display, 'layout'), "Display should have layout"
    assert display.layout in ["grid", "list"], f"Layout should be 'grid' or 'list', got {display.layout}"
    
    # Verify address structure
    address = result.address
    assert hasattr(address, 'street'), "Address should have street"
    assert hasattr(address, 'city'), "Address should have city"
    assert hasattr(address, 'state'), "Address should have state"
    assert hasattr(address, 'country'), "Address should have country"
    assert hasattr(address, 'postalCode'), "Address should have postalCode"
    assert hasattr(address, 'coordinates'), "Address should have coordinates"
    
    # Verify coordinates if present
    if address.coordinates is not None:
        assert hasattr(address.coordinates, 'latitude'), "Coordinates should have latitude"
        assert hasattr(address.coordinates, 'longitude'), "Coordinates should have longitude"
    
    # Verify metadata structure
    metadata = result.metadata
    assert hasattr(metadata, 'createdAt'), "Metadata should have createdAt"
    assert hasattr(metadata, 'updatedAt'), "Metadata should have updatedAt"
    assert hasattr(metadata, 'version'), "Metadata should have version"
    assert hasattr(metadata, 'tags'), "Metadata should have tags"
    assert hasattr(metadata, 'attributes'), "Metadata should have attributes"
    assert isinstance(metadata.tags, list), "Tags should be array"
    assert isinstance(metadata.attributes, dict), "Attributes should be map"
    
    print("✓ SimpleNested test passed")

async def test_deeply_nested():
    print("\nTesting DeeplyNested...")
    result = await baml.TestDeeplyNested("test deeply nested")
    
    # Navigate through all 5 levels
    level1 = result.level1
    assert hasattr(level1, 'data'), "Level1 should have data"
    assert hasattr(level1, 'level2'), "Level1 should have level2"
    assert isinstance(level1.data, str), "Level1 data should be string"
    
    level2 = level1.level2
    assert hasattr(level2, 'data'), "Level2 should have data"
    assert hasattr(level2, 'level3'), "Level2 should have level3"
    assert isinstance(level2.data, str), "Level2 data should be string"
    
    level3 = level2.level3
    assert hasattr(level3, 'data'), "Level3 should have data"
    assert hasattr(level3, 'level4'), "Level3 should have level4"
    assert isinstance(level3.data, str), "Level3 data should be string"
    
    level4 = level3.level4
    assert hasattr(level4, 'data'), "Level4 should have data"
    assert hasattr(level4, 'level5'), "Level4 should have level5"
    assert isinstance(level4.data, str), "Level4 data should be string"
    
    level5 = level4.level5
    assert hasattr(level5, 'data'), "Level5 should have data"
    assert hasattr(level5, 'items'), "Level5 should have items"
    assert hasattr(level5, 'mapping'), "Level5 should have mapping"
    assert isinstance(level5.data, str), "Level5 data should be string"
    assert isinstance(level5.items, list), "Level5 items should be array"
    assert isinstance(level5.mapping, dict), "Level5 mapping should be map"
    
    print("✓ DeeplyNested test passed")

async def test_complex_nested():
    print("\nTesting ComplexNested...")
    result = await baml.TestComplexNested("test complex nested")
    
    # Verify company structure
    company = result.company
    assert hasattr(company, 'id'), "Company should have id"
    assert hasattr(company, 'name'), "Company should have name"
    assert hasattr(company, 'address'), "Company should have address"
    assert hasattr(company, 'departments'), "Company should have departments"
    assert hasattr(company, 'metadata'), "Company should have metadata"
    
    # Verify departments
    assert len(company.departments) == 2, f"Expected 2 departments, got {len(company.departments)}"
    for dept in company.departments:
        assert hasattr(dept, 'id'), "Department should have id"
        assert hasattr(dept, 'name'), "Department should have name"
        assert hasattr(dept, 'manager'), "Department should have manager"
        assert hasattr(dept, 'members'), "Department should have members"
        assert hasattr(dept, 'budget'), "Department should have budget"
        assert hasattr(dept, 'projects'), "Department should have projects"
    
    # Verify employees
    assert len(result.employees) == 5, f"Expected 5 employees, got {len(result.employees)}"
    for emp in result.employees:
        assert hasattr(emp, 'id'), "Employee should have id"
        assert hasattr(emp, 'name'), "Employee should have name"
        assert hasattr(emp, 'email'), "Employee should have email"
        assert hasattr(emp, 'role'), "Employee should have role"
        assert hasattr(emp, 'department'), "Employee should have department"
        assert hasattr(emp, 'skills'), "Employee should have skills"
        assert isinstance(emp.skills, list), "Skills should be array"
    
    # Verify projects
    assert len(result.projects) == 2, f"Expected 2 projects, got {len(result.projects)}"
    for project in result.projects:
        assert hasattr(project, 'id'), "Project should have id"
        assert hasattr(project, 'name'), "Project should have name"
        assert hasattr(project, 'description'), "Project should have description"
        assert hasattr(project, 'status'), "Project should have status"
        assert hasattr(project, 'team'), "Project should have team"
        assert hasattr(project, 'milestones'), "Project should have milestones"
        assert hasattr(project, 'budget'), "Project should have budget"
        assert project.status in ["planning", "active", "completed", "cancelled"], f"Project status should be valid, got {project.status}"
        
        # Verify milestones
        for milestone in project.milestones:
            assert hasattr(milestone, 'id'), "Milestone should have id"
            assert hasattr(milestone, 'name'), "Milestone should have name"
            assert hasattr(milestone, 'dueDate'), "Milestone should have dueDate"
            assert hasattr(milestone, 'completed'), "Milestone should have completed"
            assert hasattr(milestone, 'tasks'), "Milestone should have tasks"
            
            # Verify tasks
            for task in milestone.tasks:
                assert hasattr(task, 'id'), "Task should have id"
                assert hasattr(task, 'title'), "Task should have title"
                assert hasattr(task, 'description'), "Task should have description"
                assert hasattr(task, 'assignee'), "Task should have assignee"
                assert hasattr(task, 'priority'), "Task should have priority"
                assert hasattr(task, 'status'), "Task should have status"
                assert task.priority in ["low", "medium", "high"], f"Task priority should be valid, got {task.priority}"
                assert task.status in ["todo", "in_progress", "done"], f"Task status should be valid, got {task.status}"
        
        # Verify budget
        budget = project.budget
        assert hasattr(budget, 'total'), "Budget should have total"
        assert hasattr(budget, 'spent'), "Budget should have spent"
        assert hasattr(budget, 'categories'), "Budget should have categories"
        assert hasattr(budget, 'approvals'), "Budget should have approvals"
        assert isinstance(budget.categories, dict), "Budget categories should be map"
        assert isinstance(budget.approvals, list), "Budget approvals should be array"
    
    print("✓ ComplexNested test passed")

async def test_recursive_structure():
    print("\nTesting RecursiveStructure...")
    result = await baml.TestRecursiveStructure("test recursive structure")
    
    # Verify root node
    assert hasattr(result, 'id'), "Root should have id"
    assert hasattr(result, 'name'), "Root should have name"
    assert hasattr(result, 'children'), "Root should have children"
    assert hasattr(result, 'parent'), "Root should have parent"
    assert hasattr(result, 'metadata'), "Root should have metadata"
    
    # Root should have no parent
    assert result.parent is None, "Root should have no parent"
    
    # Root should have at least 2 children
    assert len(result.children) >= 2, f"Root should have at least 2 children, got {len(result.children)}"
    
    # Verify children structure
    for child in result.children:
        assert hasattr(child, 'id'), "Child should have id"
        assert hasattr(child, 'name'), "Child should have name"
        assert hasattr(child, 'children'), "Child should have children"
        assert hasattr(child, 'parent'), "Child should have parent"
        assert hasattr(child, 'metadata'), "Child should have metadata"
        
        # Check if any children have their own children (3 levels deep)
        for grandchild in child.children:
            assert hasattr(grandchild, 'id'), "Grandchild should have id"
            assert hasattr(grandchild, 'name'), "Grandchild should have name"
            assert hasattr(grandchild, 'children'), "Grandchild should have children"
            assert hasattr(grandchild, 'parent'), "Grandchild should have parent"
            assert hasattr(grandchild, 'metadata'), "Grandchild should have metadata"
    
    print("✓ RecursiveStructure test passed")

async def main():
    # Run all tests in parallel
    tasks = [
        test_simple_nested(),
        test_deeply_nested(),
        test_complex_nested(),
        test_recursive_structure()
    ]
    
    try:
        await asyncio.gather(*tasks)
        print("\n✅ All nested structure tests passed!")
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())