// Benchmark: Deeply nested types
// Tests compiler performance with complex type hierarchies

client GPT4 {
    provider: "openai"
    model: "gpt-4"
}

// Level 1
class Address {
    street: string
    city: string
    state: string
    zip: string
}

// Level 2
class Contact {
    email: string
    phone?: string
    address: Address
}

// Level 3
class Person {
    id: string
    name: string
    age: int
    contact: Contact
}

// Level 4
class Employee {
    person: Person
    employee_id: string
    department: string
    salary: float
    manager?: Employee
}

// Level 5
class Department {
    id: string
    name: string
    head: Employee
    employees: Employee[]
    sub_departments: Department[]
}

// Level 6
class Division {
    id: string
    name: string
    departments: Department[]
    vice_president: Employee
}

// Level 7
class Company {
    id: string
    name: string
    divisions: Division[]
    ceo: Employee
    board_members: Person[]
}

// Level 8
class Conglomerate {
    id: string
    name: string
    companies: Company[]
    parent?: Conglomerate
    subsidiaries: Conglomerate[]
}

// Complex nested arrays and optionals
class Project {
    id: string
    name: string
    teams: Team[]
    milestones: Milestone[]
    budget: Budget
}

class Team {
    id: string
    name: string
    lead: Employee
    members: Employee[]
    sub_teams?: Team[]
    projects: Project[]
}

class Milestone {
    id: string
    name: string
    date: string
    deliverables: Deliverable[]
    dependencies: Milestone[]
    assigned_teams: Team[]
}

class Deliverable {
    id: string
    name: string
    description: string
    owner: Employee
    reviewers: Employee[]
    attachments?: Attachment[]
}

class Attachment {
    id: string
    filename: string
    size: int
    url: string
    uploaded_by: Employee
    permissions: Permission[]
}

class Permission {
    id: string
    type: string
    granted_to: Employee
    granted_by: Employee
    expires_at?: string
}

class Budget {
    id: string
    total: float
    allocated: float
    spent: float
    line_items: LineItem[]
    approvers: Employee[]
}

class LineItem {
    id: string
    description: string
    amount: float
    category: string
    approved_by?: Employee
    receipts?: Receipt[]
}

class Receipt {
    id: string
    amount: float
    date: string
    vendor: string
    attached_files: Attachment[]
    approved_by?: Employee
}

// Recursive and circular references
class Node {
    id: string
    value: string
    parent?: Node
    children: Node[]
    siblings?: Node[]
}

class Graph {
    nodes: Node[]
    edges: Edge[]
}

class Edge {
    id: string
    from: Node
    to: Node
    weight?: float
    metadata?: EdgeMetadata
}

class EdgeMetadata {
    created_at: string
    updated_at: string
    created_by: Person
    properties: Property[]
}

class Property {
    key: string
    value: string
    type: string
    nested_properties?: Property[]
}

// Functions using deeply nested types
function GetConglomerate(id: string) -> Conglomerate {
    client GPT4
    prompt #"Get conglomerate with all nested data: {{id}}"#
}

function CreateProject(
    name: string,
    teams: Team[],
    budget: Budget
) -> Project {
    client GPT4
    prompt #"
        Create project:
        Name: {{name}}
        Teams: {{teams}}
        Budget: {{budget}}
    "#
}

function AnalyzeGraph(graph: Graph) -> string {
    client GPT4
    prompt #"Analyze graph structure: {{graph}}"#
}

function ProcessDeliverable(
    deliverable: Deliverable,
    team: Team,
    milestone: Milestone
) -> string {
    client GPT4
    prompt #"
        Process deliverable:
        Deliverable: {{deliverable}}
        Team: {{team}}
        Milestone: {{milestone}}
    "#
}

function ValidatePermissions(
    permissions: Permission[],
    employee: Employee,
    attachment: Attachment
) -> bool {
    client GPT4
    prompt #"
        Validate permissions:
        Permissions: {{permissions}}
        Employee: {{employee}}
        Attachment: {{attachment}}
    "#
}