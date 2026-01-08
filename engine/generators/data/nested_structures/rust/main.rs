// Test file for generated BAML client
// This will be compiled against the generated baml_client module

mod baml_client;

use baml_client::sync_client::B;
use baml_client::types::*;

fn main() {
    println!("Test - baml_client module loaded successfully!");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_nested() {
        let result = B
            .TestSimpleNested
            .call("test simple nested")
            .expect("Failed to call TestSimpleNested");

        // Verify simple nested structure
        assert!(
            result.user.id > 0,
            "Expected user.id to be positive, got {}",
            result.user.id
        );
        assert!(
            !result.user.name.is_empty(),
            "Expected user.name to be non-empty"
        );
        assert!(
            !result.user.profile.bio.is_empty(),
            "Expected user.profile.bio to be non-empty"
        );
        assert!(
            !result.user.profile.avatar.is_empty(),
            "Expected user.profile.avatar to be non-empty"
        );

        // Verify theme is valid (light or dark)
        assert!(
            matches!(
                result.user.profile.preferences.theme,
                Union2KdarkOrKlight::Klight | Union2KdarkOrKlight::Kdark
            ),
            "Expected user.profile.preferences.theme to be 'light' or 'dark'"
        );

        assert!(
            !result.user.profile.preferences.language.is_empty(),
            "Expected user.profile.preferences.language to be non-empty"
        );

        // Verify notification frequency is valid
        assert!(
            matches!(
                result.user.profile.preferences.notifications.frequency,
                Union3KdailyOrKimmediateOrKweekly::Kimmediate
                    | Union3KdailyOrKimmediateOrKweekly::Kdaily
                    | Union3KdailyOrKimmediateOrKweekly::Kweekly
            ),
            "Expected notifications.frequency to be valid"
        );

        // Verify profile visibility is valid
        assert!(
            matches!(
                result.user.settings.privacy.profileVisibility,
                Union3KfriendsOrKprivateOrKpublic::Kpublic
                    | Union3KfriendsOrKprivateOrKpublic::Kprivate
                    | Union3KfriendsOrKprivateOrKpublic::Kfriends
            ),
            "Expected privacy.profileVisibility to be valid"
        );

        assert!(
            result.user.settings.display.fontSize > 0,
            "Expected display.fontSize to be positive, got {}",
            result.user.settings.display.fontSize
        );
        assert!(
            !result.user.settings.display.colorScheme.is_empty(),
            "Expected display.colorScheme to be non-empty"
        );

        // Verify address
        assert!(
            !result.address.street.is_empty(),
            "Expected address.street to be non-empty"
        );
        assert!(
            !result.address.city.is_empty(),
            "Expected address.city to be non-empty"
        );
        assert!(
            !result.address.state.is_empty(),
            "Expected address.state to be non-empty"
        );
        assert!(
            !result.address.country.is_empty(),
            "Expected address.country to be non-empty"
        );
        assert!(
            !result.address.postalCode.is_empty(),
            "Expected address.postalCode to be non-empty"
        );

        // Verify metadata
        assert!(
            !result.metadata.createdAt.is_empty(),
            "Expected metadata.createdAt to be non-empty"
        );
        assert!(
            !result.metadata.updatedAt.is_empty(),
            "Expected metadata.updatedAt to be non-empty"
        );
        assert!(
            result.metadata.version > 0,
            "Expected metadata.version to be positive, got {}",
            result.metadata.version
        );
        // tags and attributes are valid if they exist (Vec and HashMap)
    }

    #[test]
    fn test_deeply_nested() {
        let result = B
            .TestDeeplyNested
            .call("test deeply nested")
            .expect("Failed to call TestDeeplyNested");

        // Verify deeply nested structure (5 levels deep)
        assert!(
            !result.level1.data.is_empty(),
            "Expected level1.data to be non-empty"
        );
        assert!(
            !result.level1.level2.data.is_empty(),
            "Expected level1.level2.data to be non-empty"
        );
        assert!(
            !result.level1.level2.level3.data.is_empty(),
            "Expected level1.level2.level3.data to be non-empty"
        );
        assert!(
            !result.level1.level2.level3.level4.data.is_empty(),
            "Expected level1.level2.level3.level4.data to be non-empty"
        );
        assert!(
            !result.level1.level2.level3.level4.level5.data.is_empty(),
            "Expected level1.level2.level3.level4.level5.data to be non-empty"
        );
        // items and mapping are valid if they exist (Vec and HashMap)
    }

    #[test]
    fn test_complex_nested() {
        let result = B
            .TestComplexNested
            .call("test complex nested")
            .expect("Failed to call TestComplexNested");

        // Verify complex nested structure
        assert!(
            result.company.id > 0,
            "Expected company.id to be positive, got {}",
            result.company.id
        );
        assert!(
            !result.company.name.is_empty(),
            "Expected company.name to be non-empty"
        );
        assert_eq!(
            result.company.departments.len(),
            2,
            "Expected 2 departments, got {}",
            result.company.departments.len()
        );
        assert!(
            !result.company.metadata.founded.is_empty(),
            "Expected company.metadata.founded to be non-empty"
        );
        assert!(
            !result.company.metadata.industry.is_empty(),
            "Expected company.metadata.industry to be non-empty"
        );

        // Verify company size is valid
        assert!(
            matches!(
                result.company.metadata.size,
                Union4KenterpriseOrKlargeOrKmediumOrKsmall::Ksmall
                    | Union4KenterpriseOrKlargeOrKmediumOrKsmall::Kmedium
                    | Union4KenterpriseOrKlargeOrKmediumOrKsmall::Klarge
                    | Union4KenterpriseOrKlargeOrKmediumOrKsmall::Kenterprise
            ),
            "Expected company.metadata.size to be valid"
        );

        // Verify departments
        for (i, dept) in result.company.departments.iter().enumerate() {
            assert!(dept.id > 0, "Department {} has invalid id: {}", i, dept.id);
            assert!(!dept.name.is_empty(), "Department {} has empty name", i);
            assert!(
                dept.budget > 0.0,
                "Department {} has invalid budget: {}",
                i,
                dept.budget
            );
        }

        // Verify employees
        assert_eq!(
            result.employees.len(),
            5,
            "Expected 5 employees, got {}",
            result.employees.len()
        );
        for (i, emp) in result.employees.iter().enumerate() {
            assert!(emp.id > 0, "Employee {} has invalid id: {}", i, emp.id);
            assert!(!emp.name.is_empty(), "Employee {} has empty name", i);
            assert!(!emp.email.is_empty(), "Employee {} has empty email", i);
            assert!(!emp.role.is_empty(), "Employee {} has empty role", i);
            assert!(
                !emp.department.is_empty(),
                "Employee {} has empty department",
                i
            );
        }

        // Verify projects
        assert_eq!(
            result.projects.len(),
            2,
            "Expected 2 projects, got {}",
            result.projects.len()
        );
        for (i, proj) in result.projects.iter().enumerate() {
            assert!(proj.id > 0, "Project {} has invalid id: {}", i, proj.id);
            assert!(!proj.name.is_empty(), "Project {} has empty name", i);
            assert!(
                !proj.description.is_empty(),
                "Project {} has empty description",
                i
            );

            // Verify project status is valid
            assert!(
                matches!(
                    proj.status,
                    Union4KactiveOrKcancelledOrKcompletedOrKplanning::Kplanning
                        | Union4KactiveOrKcancelledOrKcompletedOrKplanning::Kactive
                        | Union4KactiveOrKcancelledOrKcompletedOrKplanning::Kcompleted
                        | Union4KactiveOrKcancelledOrKcompletedOrKplanning::Kcancelled
                ),
                "Project {} has invalid status",
                i
            );

            assert!(
                proj.budget.total > 0.0,
                "Project {} has invalid budget total: {}",
                i,
                proj.budget.total
            );
            assert!(
                proj.budget.spent >= 0.0,
                "Project {} has invalid budget spent: {}",
                i,
                proj.budget.spent
            );
            // categories is valid if it exists (HashMap)
        }
    }

    #[test]
    fn test_recursive_structure() {
        let result = B
            .TestRecursiveStructure
            .call("test recursive structure")
            .expect("Failed to call TestRecursiveStructure");

        // Verify recursive structure
        assert!(
            result.id > 0,
            "Expected root.id to be positive, got {}",
            result.id
        );
        assert!(
            !result.name.is_empty(),
            "Expected root.name to be non-empty"
        );
        assert!(
            result.children.len() >= 2,
            "Expected at least 2 children, got {}",
            result.children.len()
        );

        // Verify children have proper structure
        for (i, child) in result.children.iter().enumerate() {
            assert!(child.id > 0, "Child {} has invalid id: {}", i, child.id);
            assert!(!child.name.is_empty(), "Child {} has empty name", i);
            if let Some(ref parent) = child.parent {
                assert_eq!(
                    parent.id, result.id,
                    "Child {} parent id mismatch: expected {}, got {}",
                    i, result.id, parent.id
                );
            }
        }

        // Check for 3-level depth (at least one child should have grandchildren)
        let has_grandchildren = result
            .children
            .iter()
            .any(|child| !child.children.is_empty());
        assert!(
            has_grandchildren,
            "Expected at least one child to have children (3 levels deep)"
        );
    }
}
