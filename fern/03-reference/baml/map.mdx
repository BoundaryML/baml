Map values (AKA Dictionaries) allow you to store key-value pairs.

<Tip>Most of BAML (clients, tests, classes, etc) is represented as a map.</Tip>

## Syntax

To declare a map in a BAML file, you can use the following syntax:

```baml
{
  key1 value1,
  key2 {
    nestedKey1 nestedValue1,
    nestedKey2 nestedValue2
  }
}
```

### Key Points:
- **Colons**: Not used in BAML maps; keys and values are separated by spaces.
- **Value Types**: Maps can contain unquoted or quoted strings, booleans, numbers, and nested maps as values.
- **Classes**: Classes in BAML are represented as maps with keys and values.

## Usage Examples

### Example 1: Simple Map

```baml

class Person {
    name string
    age int
    isEmployed bool
}

function DescribePerson(person: Person) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
        Describe the person with the following details: {{ person }}.
    "#
}

test PersonDescription {
    functions [DescribePerson]
    args { 
        person {
            name "John Doe",
            age 30,
            isEmployed true
        }
    }
}
```

### Example 2: Nested Map

```baml

class Company {
    name string
    location map<string, string>
    employeeCount int
}

function DescribeCompany(company: Company) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
        Describe the company with the following details: {{ company }}.
    "#
}

test CompanyDescription {
    functions [DescribeCompany]
    args { 
        company {
            name "TechCorp",
            location {
                city "San Francisco",
                state "California"
            },
            employeeCount 500
        }
    }
}
```

### Example 3: Map with Multiline String

```baml
class Project {
    title string
    description string
}

function DescribeProject(project: Project) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
        Describe the project with the following details: {{ project }}.
    "#
}

test ProjectDescription {
    functions [DescribeProject]
    args { 
        project {
            title "AI Research",
            description #"
                This project focuses on developing
                advanced AI algorithms to improve
                machine learning capabilities.
            "#
        }
    }
}
```
