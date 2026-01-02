"""
E2E Tests for TypeBuilder and dynamic types.
Mirrors the Rust e2e tests in ../rust/main.rs
"""

import asyncio
import sys
sys.path.insert(0, '..')

from baml_client import b
from baml_client.type_builder import TypeBuilder


async def test_dynamic_class_property_e2e():
    """Test adding property to dynamic class and calling LLM."""
    print("\n=== test_dynamic_class_property_e2e ===")

    tb = TypeBuilder()

    # Add dynamic property "occupation" to Person class
    tb.Person.add_property("occupation", tb.string()).description(
        "The person's job or profession"
    )

    # Call function with TypeBuilder
    result = await b.GetPerson(
        "A software engineer named Alice who is 30 years old and works as a backend developer",
        {"tb": tb}
    )

    # Verify static fields
    assert result.name, "Name should not be empty"
    assert result.age > 0, "Age should be positive"

    # Verify dynamic field
    assert hasattr(result, 'occupation') or 'occupation' in result.model_dump(), \
        "Person should have dynamic 'occupation' field"

    occupation = result.model_dump().get('occupation', getattr(result, 'occupation', None))
    assert occupation, f"Occupation should not be empty: got '{occupation}'"

    print(f"Got person: {result.name} (age {result.age}), occupation: {occupation}")


async def test_multiple_dynamic_properties_e2e():
    """Test adding multiple properties with different types."""
    print("\n=== test_multiple_dynamic_properties_e2e ===")

    tb = TypeBuilder()

    tb.Person.add_property("email", tb.string()).description("Email address")
    tb.Person.add_property("is_employed", tb.bool()).description("Whether currently employed")
    tb.Person.add_property("years_experience", tb.int()).description("Years of work experience")

    result = await b.GetPerson(
        "Bob Smith, age 35, email bob@example.com, currently employed with 10 years experience",
        {"tb": tb}
    )

    data = result.model_dump()

    # Verify static fields
    assert result.name, "Name should not be empty"
    assert result.age > 0, "Age should be positive"

    # Verify all dynamic fields
    assert "email" in data, "Should have email"
    assert "is_employed" in data, "Should have is_employed"
    assert "years_experience" in data, "Should have years_experience"

    print(f"Person: {result.name}, email: {data['email']}, employed: {data['is_employed']}, years: {data['years_experience']}")


async def test_dynamic_enum_value_e2e():
    """Test adding enum values and classifying correctly."""
    print("\n=== test_dynamic_enum_value_e2e ===")

    tb = TypeBuilder()

    # Add new enum values to Category
    tb.Category.add_value("Sports").description("Sports and athletics news")
    tb.Category.add_value("Politics").description("Political news and government")
    tb.Category.add_value("Entertainment").description("Movies, TV, celebrities")

    result = await b.ClassifyArticle(
        "The Lakers won the championship last night with a stunning 3-pointer in overtime",
        {"tb": tb}
    )

    category_str = str(result)
    print(f"Category: {category_str}")

    # Should be one of our categories
    valid_categories = ["Sports", "Technology", "Science", "Arts", "Politics", "Entertainment"]
    assert category_str in valid_categories, f"Should be a valid category, got: {category_str}"


async def test_nested_dynamic_types_e2e():
    """Test handling nested dynamic types."""
    print("\n=== test_nested_dynamic_types_e2e ===")

    tb = TypeBuilder()

    # Add dynamic property to Person (nested in Article)
    tb.Person.add_property("bio", tb.string())

    # Add dynamic properties to Article
    tb.Article.add_property("word_count", tb.int())
    tb.Article.add_property("published", tb.bool())

    # Add new category value
    tb.Category.add_value("Business")

    result = await b.CreateArticle(
        "A 500-word published article about tech startups by John Doe, a tech journalist",
        {"tb": tb}
    )

    data = result.model_dump()
    author_data = result.author.model_dump() if hasattr(result.author, 'model_dump') else result.author

    # Verify static fields
    assert result.title, "Title should not be empty"
    assert result.author.name, "Author name should exist"

    # Verify dynamic fields on Article
    assert "word_count" in data, "Article should have word_count"
    assert "published" in data, "Article should have published"

    # Verify dynamic field on nested Person
    assert "bio" in author_data, "Author should have bio"

    print(f"Article: {result.title} by {result.author.name} (category: {result.category})")


async def test_complex_dynamic_types_e2e():
    """Test handling lists and optionals in dynamic properties."""
    print("\n=== test_complex_dynamic_types_e2e ===")

    tb = TypeBuilder()

    # Add list of strings
    tb.Person.add_property("skills", tb.string().list())

    # Add optional string
    tb.Person.add_property("nickname", tb.string().optional()).description(
        "The person's nickname (if explicitly provided)"
    )

    result = await b.GetPerson(
        "Alice Johnson, 28, skills: Rust, Python, Go. Nickname: AJ",
        {"tb": tb}
    )

    data = result.model_dump()

    print(f"Person: {result.name} (age {result.age})")

    # Check skills list
    assert "skills" in data, "Person should have skills"
    skills = data["skills"]
    assert isinstance(skills, list), "Skills should be a list"
    assert len(skills) == 3, f"Skills should have 3 items, got {len(skills)}"

    # Check optional nickname
    assert "nickname" in data, "Person should have nickname"
    assert data["nickname"] == "AJ", f"Nickname should be AJ, got {data['nickname']}"


async def test_alias_e2e():
    """Test using alias for better LLM matching."""
    print("\n=== test_alias_e2e ===")

    tb = TypeBuilder()

    # Add a category with an alias
    tb.Category.add_value("AI").alias("Artificial Intelligence").description(
        "Artificial intelligence and machine learning"
    )

    result = await b.ClassifyArticle(
        "GPT-5 achieves human-level reasoning in new benchmarks, researchers claim",
        {"tb": tb}
    )

    category_str = str(result)
    print(f"Category for AI article: {category_str}")

    # Should be AI or Technology
    assert category_str in ["AI", "Technology"], f"Expected AI-related category, got: {category_str}"


async def test_fully_dynamic_class_e2e():
    """Test creating completely new class at runtime."""
    print("\n=== test_fully_dynamic_class_e2e ===")

    tb = TypeBuilder()

    # Create a completely new class at runtime
    product_class = tb.add_class("Product")
    product_class.add_property("name", tb.string())
    product_class.add_property("price", tb.float())
    product_class.add_property("in_stock", tb.bool())

    # Verify the type is registered
    product_type = product_class.type()
    print(f"Created dynamic Product type")

    # Note: Can't call a function that returns Product directly
    # because it's not in the schema, but we verify the type exists
    assert product_type is not None


async def main():
    """Run all e2e tests."""
    print("Running TypeBuilder E2E Tests")
    print("=" * 50)

    tests = [
        test_dynamic_class_property_e2e,
        test_multiple_dynamic_properties_e2e,
        test_dynamic_enum_value_e2e,
        test_nested_dynamic_types_e2e,
        test_complex_dynamic_types_e2e,
        test_alias_e2e,
        test_fully_dynamic_class_e2e,
    ]

    passed = 0
    failed = 0

    for test in tests:
        try:
            await test()
            passed += 1
            print(f"✓ {test.__name__} PASSED")
        except Exception as e:
            failed += 1
            print(f"✗ {test.__name__} FAILED: {e}")

    print("\n" + "=" * 50)
    print(f"Results: {passed} passed, {failed} failed")

    if failed > 0:
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())
