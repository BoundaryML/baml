// Test suite for dynamic types and TypeBuilder codegen
// This file tests @@dynamic classes and enums

// A dynamic class that can be extended at runtime
class Person {
    name string
    age int
    @@dynamic
}

// A non-dynamic class for comparison
class Address {
    street string
    city string
    country string
}

// A purely dynamic class with no static fields
class PureDynamic {
    @@dynamic
}

// A dynamic enum that can have values added at runtime
enum Category {
    Technology
    Science
    Arts
    @@dynamic
}

// A non-dynamic enum for comparison
enum Status {
    Active
    Inactive
    Pending
}

// A class that uses both dynamic and non-dynamic types
class Article {
    title string
    category Category
    author Person
    status Status
    @@dynamic
}

// A dynamic enum with descriptions
enum Priority {
    High @description("Urgent items")
    Medium @description("Normal priority")
    Low @description("Can wait")
    @@dynamic
}

// Client for testing
client<llm> TestClient {
    provider "openai"
    options {
        model "gpt-4o-mini"
    }
}

// Function that returns a dynamic class
function GetPerson(description: string) -> Person {
    client TestClient
    prompt #"
        Based on this description, create a person:
        {{ description }}

        {{ ctx.output_format }}
    "#
}

// Function that returns a dynamic enum
function ClassifyArticle(text: string) -> Category {
    client TestClient
    prompt #"
        Classify this text into a category:
        {{ text }}

        {{ ctx.output_format }}
    "#
}

// Function that uses multiple dynamic types
function CreateArticle(topic: string) -> Article {
    client TestClient
    prompt #"
        Create an article about: {{ topic }}

        {{ ctx.output_format }}
    "#
}

// Function that returns a pure dynamic type (no static fields)
function GetDynamicResponse(prompt: string) -> PureDynamic {
    client TestClient
    prompt #"
        {{ prompt }}

        {{ ctx.output_format }}
    "#
}

test GetPersonTest {
    functions [GetPerson]
    args {
        description "A software engineer named Alice who is 30 years old"
    }
}

test ClassifyArticleTest {
    functions [ClassifyArticle]
    args {
        text "The new quantum computer can solve problems 100x faster"
    }
}
