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
    fn test_kitchen_sink() {
        let result = B
            .TestKitchenSink
            .call("test kitchen sink")
            .expect("Failed to call TestKitchenSink");

        // Basic field validations
        assert!(
            result.id > 0,
            "Expected id to be positive, got {}",
            result.id
        );
        assert!(!result.name.is_empty(), "Expected name to be non-empty");
        assert!(
            result.score > 0.0,
            "Expected score to be positive, got {}",
            result.score
        );
        // nothing field is () which is always "null" in Rust

        // Verify literal union fields using match
        let status_valid = matches!(
            result.status,
            Union3KarchivedOrKdraftOrKpublished::Kdraft
                | Union3KarchivedOrKdraftOrKpublished::Kpublished
                | Union3KarchivedOrKdraftOrKpublished::Karchived
        );
        assert!(status_valid, "Expected status to be a valid literal type");

        let priority_valid = matches!(
            result.priority,
            Union5IntK1OrIntK2OrIntK3OrIntK4OrIntK5::IntK1
                | Union5IntK1OrIntK2OrIntK3OrIntK4OrIntK5::IntK2
                | Union5IntK1OrIntK2OrIntK3OrIntK4OrIntK5::IntK3
                | Union5IntK1OrIntK2OrIntK3OrIntK4OrIntK5::IntK4
                | Union5IntK1OrIntK2OrIntK3OrIntK4OrIntK5::IntK5
        );
        assert!(priority_valid, "Expected priority to be between 1-5");

        // Verify union fields
        let data_valid = matches!(
            result.data,
            Union3DataObjectOrIntOrString::String(_)
                | Union3DataObjectOrIntOrString::Int(_)
                | Union3DataObjectOrIntOrString::DataObject(_)
        );
        assert!(data_valid, "Expected data union to have a valid type");

        let result_valid = matches!(
            result.result,
            Union2ErrorOrSuccess::Success(_) | Union2ErrorOrSuccess::Error(_)
        );
        assert!(result_valid, "Expected result union to have a valid type");

        // Verify arrays (just check they exist - length >= 0 is always true)
        // tags and numbers are Vec types which always exist

        // Verify maps (HashMap types always exist)
        // metadata and scores are HashMap types

        // Verify nested objects
        assert!(
            result.user.id > 0,
            "Expected user.id to be positive, got {}",
            result.user.id
        );
        assert!(
            !result.user.profile.name.is_empty(),
            "Expected user.profile.name to be non-empty"
        );
        assert!(
            !result.user.profile.email.is_empty(),
            "Expected user.profile.email to be non-empty"
        );
    }

    #[test]
    fn test_ultra_complex() {
        let result = B
            .TestUltraComplex
            .call("test ultra complex")
            .expect("Failed to call TestUltraComplex");

        // Basic validations
        assert!(
            result.tree.id > 0,
            "Expected tree.id to be positive, got {}",
            result.tree.id
        );

        // Verify tree union types using match
        let tree_type_valid = matches!(
            result.tree.r#type,
            Union2KbranchOrKleaf::Kleaf | Union2KbranchOrKleaf::Kbranch
        );
        assert!(
            tree_type_valid,
            "Expected tree.type to be 'leaf' or 'branch'"
        );

        let tree_value_valid = matches!(
            result.tree.value,
            Union4IntOrListNodeOrMapStringKeyNodeValueOrString::String(_)
                | Union4IntOrListNodeOrMapStringKeyNodeValueOrString::Int(_)
                | Union4IntOrListNodeOrMapStringKeyNodeValueOrString::ListNode(_)
                | Union4IntOrListNodeOrMapStringKeyNodeValueOrString::MapStringKeyNodeValue(_)
        );
        assert!(tree_value_valid, "Expected tree.value to have a valid type");

        // Verify widgets
        assert!(
            !result.widgets.is_empty(),
            "Expected at least 1 widget, got {}",
            result.widgets.len()
        );

        // Verify widget types using match
        for (i, widget) in result.widgets.iter().enumerate() {
            let widget_type_valid = matches!(
                widget.r#type,
                Union4KbuttonOrKcontainerOrKimageOrKtext::Kbutton
                    | Union4KbuttonOrKcontainerOrKimageOrKtext::Ktext
                    | Union4KbuttonOrKcontainerOrKimageOrKtext::Kimage
                    | Union4KbuttonOrKcontainerOrKimageOrKtext::Kcontainer
            );
            assert!(widget_type_valid, "Widget {} has invalid type", i);

            // Verify appropriate widget fields are populated based on type
            match &widget.r#type {
                Union4KbuttonOrKcontainerOrKimageOrKtext::Kbutton => {
                    assert!(
                        widget.button.is_some(),
                        "Button widget {} missing button data",
                        i
                    );
                }
                Union4KbuttonOrKcontainerOrKimageOrKtext::Ktext => {
                    assert!(widget.text.is_some(), "Text widget {} missing text data", i);
                    if let Some(ref text) = widget.text {
                        let format_valid = matches!(
                            text.format,
                            Union3KhtmlOrKmarkdownOrKplain::Kplain
                                | Union3KhtmlOrKmarkdownOrKplain::Kmarkdown
                                | Union3KhtmlOrKmarkdownOrKplain::Khtml
                        );
                        assert!(format_valid, "Text widget {} has invalid format", i);
                    }
                }
                Union4KbuttonOrKcontainerOrKimageOrKtext::Kimage => {
                    assert!(
                        widget.img.is_some(),
                        "Image widget {} missing image data",
                        i
                    );
                }
                Union4KbuttonOrKcontainerOrKimageOrKtext::Kcontainer => {
                    assert!(
                        widget.container.is_some(),
                        "Container widget {} missing container data",
                        i
                    );
                }
            }
        }
    }

    #[test]
    fn test_recursive_complexity() {
        let result = B
            .TestRecursiveComplexity
            .call("test recursive complexity")
            .expect("Failed to call TestRecursiveComplexity");

        // Basic validations
        assert!(
            result.id > 0,
            "Expected node.id to be positive, got {}",
            result.id
        );

        // Verify node union types using match
        let node_type_valid = matches!(
            result.r#type,
            Union2KbranchOrKleaf::Kleaf | Union2KbranchOrKleaf::Kbranch
        );
        assert!(
            node_type_valid,
            "Expected node.type to be 'leaf' or 'branch'"
        );

        let node_value_valid = matches!(
            result.value,
            Union4IntOrListNodeOrMapStringKeyNodeValueOrString::String(_)
                | Union4IntOrListNodeOrMapStringKeyNodeValueOrString::Int(_)
                | Union4IntOrListNodeOrMapStringKeyNodeValueOrString::ListNode(_)
                | Union4IntOrListNodeOrMapStringKeyNodeValueOrString::MapStringKeyNodeValue(_)
        );
        assert!(node_value_valid, "Expected node.value to have a valid type");
    }
}
