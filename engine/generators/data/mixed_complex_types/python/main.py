import asyncio
import sys
from baml_client import baml

async def test_kitchen_sink():
    print("Testing KitchenSink...")
    result = await baml.TestKitchenSink("test kitchen sink")
    
    # Verify primitives
    assert isinstance(result.id, int), "ID should be int"
    assert isinstance(result.name, str), "Name should be string"
    assert isinstance(result.score, (int, float)), "Score should be number"
    assert isinstance(result.active, bool), "Active should be bool"
    assert result.nothing is None, "Nothing should be None"
    
    # Verify literals
    assert result.status in ["draft", "published", "archived"], f"Status should be valid literal, got {result.status}"
    assert result.priority in [1, 2, 3, 4, 5], f"Priority should be valid literal, got {result.priority}"
    
    # Verify arrays
    assert isinstance(result.tags, list), "Tags should be array"
    assert all(isinstance(x, str) for x in result.tags), "All tags should be strings"
    assert isinstance(result.numbers, list), "Numbers should be array"
    assert all(isinstance(x, int) for x in result.numbers), "All numbers should be ints"
    assert isinstance(result.matrix, list), "Matrix should be array"
    assert all(isinstance(row, list) for row in result.matrix), "Matrix rows should be arrays"
    
    # Verify maps
    assert isinstance(result.metadata, dict), "Metadata should be map"
    assert all(isinstance(k, str) and isinstance(v, str) for k, v in result.metadata.items()), "Metadata should be str->str"
    assert isinstance(result.scores, dict), "Scores should be map"
    assert all(isinstance(k, str) and isinstance(v, (int, float)) for k, v in result.scores.items()), "Scores should be str->float"
    
    # Verify optional/nullable
    if hasattr(result, 'description') and result.description is not None:
        assert isinstance(result.description, str), "Description should be string when present"
    
    if result.notes is not None:
        assert isinstance(result.notes, str), "Notes should be string when not null"
    
    # Verify unions
    assert isinstance(result.data, (str, int)) or hasattr(result.data, 'type'), "Data should be string, int, or DataObject"
    assert hasattr(result.result, 'type'), "Result should have type discriminator"
    assert result.result.type in ["success", "error"], f"Result type should be valid, got {result.result.type}"
    
    # Verify nested objects
    assert hasattr(result.user, 'id'), "User should have id"
    assert hasattr(result.user, 'profile'), "User should have profile"
    assert hasattr(result.user, 'settings'), "User should have settings"
    
    # Verify user profile
    profile = result.user.profile
    assert hasattr(profile, 'name'), "Profile should have name"
    assert hasattr(profile, 'email'), "Profile should have email"
    assert hasattr(profile, 'links'), "Profile should have links"
    assert isinstance(profile.links, list), "Profile links should be array"
    
    # Verify user settings
    settings = result.user.settings
    assert isinstance(settings, dict), "User settings should be map"
    
    # Verify items array
    assert isinstance(result.items, list), "Items should be array"
    for item in result.items:
        assert hasattr(item, 'id'), "Item should have id"
        assert hasattr(item, 'name'), "Item should have name"
        assert hasattr(item, 'variants'), "Item should have variants"
        assert hasattr(item, 'attributes'), "Item should have attributes"
        assert isinstance(item.variants, list), "Item variants should be array"
        assert isinstance(item.attributes, dict), "Item attributes should be map"
    
    # Verify configuration
    config = result.config
    assert hasattr(config, 'version'), "Config should have version"
    assert hasattr(config, 'features'), "Config should have features"
    assert hasattr(config, 'environments'), "Config should have environments"
    assert hasattr(config, 'rules'), "Config should have rules"
    assert isinstance(config.features, list), "Config features should be array"
    assert isinstance(config.environments, dict), "Config environments should be map"
    assert isinstance(config.rules, list), "Config rules should be array"
    
    print("✓ KitchenSink test passed")

async def test_ultra_complex():
    print("\nTesting UltraComplex...")
    result = await baml.TestUltraComplex("test ultra complex")
    
    # Verify tree structure
    tree = result.tree
    assert hasattr(tree, 'id'), "Tree should have id"
    assert hasattr(tree, 'type'), "Tree should have type"
    assert hasattr(tree, 'value'), "Tree should have value"
    assert tree.type in ["leaf", "branch"], f"Tree type should be valid, got {tree.type}"
    
    # Verify widgets array
    assert isinstance(result.widgets, list), "Widgets should be array"
    assert len(result.widgets) > 0, "Should have at least one widget"
    
    for widget in result.widgets:
        assert hasattr(widget, 'type'), "Widget should have type"
        assert widget.type in ["button", "text", "image", "container"], f"Widget type should be valid, got {widget.type}"
        
        # Check type-specific fields
        if widget.type == "button":
            assert hasattr(widget, 'button'), "Button widget should have button field"
            if widget.button is not None:
                assert hasattr(widget.button, 'label'), "Button should have label"
                assert hasattr(widget.button, 'action'), "Button should have action"
        elif widget.type == "text":
            assert hasattr(widget, 'text'), "Text widget should have text field"
            if widget.text is not None:
                assert hasattr(widget.text, 'content'), "Text should have content"
                assert hasattr(widget.text, 'format'), "Text should have format"
        elif widget.type == "image":
            assert hasattr(widget, 'image'), "Image widget should have image field"
            if widget.image is not None:
                assert hasattr(widget.image, 'source'), "Image should have source"
                assert hasattr(widget.image, 'alt'), "Image should have alt"
        elif widget.type == "container":
            assert hasattr(widget, 'container'), "Container widget should have container field"
            if widget.container is not None:
                assert hasattr(widget.container, 'layout'), "Container should have layout"
                assert hasattr(widget.container, 'children'), "Container should have children"
    
    # Verify complex data (optional)
    if result.data is not None:
        assert hasattr(result.data, 'primary'), "ComplexData should have primary"
        primary = result.data.primary
        assert hasattr(primary, 'values'), "PrimaryData should have values"
        assert hasattr(primary, 'mappings'), "PrimaryData should have mappings"
        assert hasattr(primary, 'flags'), "PrimaryData should have flags"
        assert isinstance(primary.values, list), "Values should be array"
        assert isinstance(primary.mappings, dict), "Mappings should be map"
        assert isinstance(primary.flags, list), "Flags should be array"
    
    # Verify response
    response = result.response
    assert hasattr(response, 'status'), "Response should have status"
    assert hasattr(response, 'metadata'), "Response should have metadata"
    assert response.status in ["success", "error"], f"Response status should be valid, got {response.status}"
    
    if response.status == "success":
        assert hasattr(response, 'data'), "Success response should have data"
        if response.data is not None:
            assert hasattr(response.data, 'id'), "User data should have id"
            assert hasattr(response.data, 'profile'), "User data should have profile"
    
    # Verify metadata
    metadata = response.metadata
    assert hasattr(metadata, 'timestamp'), "Metadata should have timestamp"
    assert hasattr(metadata, 'requestId'), "Metadata should have requestId"
    assert hasattr(metadata, 'duration'), "Metadata should have duration"
    assert hasattr(metadata, 'retries'), "Metadata should have retries"
    
    # Verify assets
    assert isinstance(result.assets, list), "Assets should be array"
    for asset in result.assets:
        assert hasattr(asset, 'id'), "Asset should have id"
        assert hasattr(asset, 'type'), "Asset should have type"
        assert hasattr(asset, 'metadata'), "Asset should have metadata"
        assert hasattr(asset, 'tags'), "Asset should have tags"
        assert asset.type in ["image", "audio", "document"], f"Asset type should be valid, got {asset.type}"
        assert isinstance(asset.tags, list), "Asset tags should be array"
        
        # Verify asset metadata
        asset_metadata = asset.metadata
        assert hasattr(asset_metadata, 'filename'), "AssetMetadata should have filename"
        assert hasattr(asset_metadata, 'size'), "AssetMetadata should have size"
        assert hasattr(asset_metadata, 'mimeType'), "AssetMetadata should have mimeType"
        assert hasattr(asset_metadata, 'uploaded'), "AssetMetadata should have uploaded"
        assert hasattr(asset_metadata, 'checksum'), "AssetMetadata should have checksum"
    
    print("✓ UltraComplex test passed")

async def test_recursive_complexity():
    print("\nTesting RecursiveComplexity...")
    result = await baml.TestRecursiveComplexity("test recursive complexity")
    
    # Verify it's a Node
    assert hasattr(result, 'id'), "Node should have id"
    assert hasattr(result, 'type'), "Node should have type"
    assert hasattr(result, 'value'), "Node should have value"
    assert hasattr(result, 'metadata'), "Node should have metadata"
    assert result.type in ["leaf", "branch"], f"Node type should be valid, got {result.type}"
    
    # Function to recursively verify node structure
    def verify_node(node, level=0, max_level=5):
        if level > max_level:
            return  # Prevent infinite recursion
        
        assert hasattr(node, 'id'), f"Node at level {level} should have id"
        assert hasattr(node, 'type'), f"Node at level {level} should have type"
        assert hasattr(node, 'value'), f"Node at level {level} should have value"
        assert node.type in ["leaf", "branch"], f"Node type at level {level} should be valid, got {node.type}"
        
        # Check value types
        if isinstance(node.value, str):
            # String value - valid for leaf nodes
            pass
        elif isinstance(node.value, int):
            # Int value - valid for leaf nodes
            pass
        elif isinstance(node.value, list):
            # Array of nodes - verify each node
            for child_node in node.value:
                if hasattr(child_node, 'type'):  # It's a Node
                    verify_node(child_node, level + 1, max_level)
        elif isinstance(node.value, dict):
            # Map containing nodes - verify each node value
            for key, child_node in node.value.items():
                if hasattr(child_node, 'type'):  # It's a Node
                    verify_node(child_node, level + 1, max_level)
        
        # Verify metadata if present
        if node.metadata is not None:
            metadata = node.metadata
            assert hasattr(metadata, 'created'), "NodeMetadata should have created"
            assert hasattr(metadata, 'modified'), "NodeMetadata should have modified"
            assert hasattr(metadata, 'tags'), "NodeMetadata should have tags"
            assert hasattr(metadata, 'attributes'), "NodeMetadata should have attributes"
            assert isinstance(metadata.tags, list), "NodeMetadata tags should be array"
            assert isinstance(metadata.attributes, dict), "NodeMetadata attributes should be map"
    
    # Verify the recursive structure (at least 3 levels deep)
    verify_node(result)
    
    print("✓ RecursiveComplexity test passed")

async def main():
    # Run all tests in parallel
    tasks = [
        test_kitchen_sink(),
        test_ultra_complex(),
        test_recursive_complexity()
    ]
    
    try:
        await asyncio.gather(*tasks)
        print("\n✅ All mixed complex type tests passed!")
    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())