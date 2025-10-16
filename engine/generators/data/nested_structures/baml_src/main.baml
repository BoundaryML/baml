// Test deeply nested structures in BAML

class SimpleNested {
  user User
  address Address
  metadata Metadata
}

class User {
  id int
  name string
  profile Profile
  settings UserSettings
}

class Profile {
  bio string
  avatar string
  social SocialLinks
  preferences Preferences
}

class SocialLinks {
  twitter string?
  github string?
  linkedin string?
  website string?
}

class Preferences {
  theme "light" | "dark"
  language string
  notifications NotificationSettings
}

class NotificationSettings {
  email bool
  push bool
  sms bool
  frequency "immediate" | "daily" | "weekly"
}

class UserSettings {
  privacy PrivacySettings
  display DisplaySettings
  advanced map<string, string>
}

class PrivacySettings {
  profileVisibility "public" | "private" | "friends"
  showEmail bool
  showPhone bool
}

class DisplaySettings {
  fontSize int
  colorScheme string
  layout "grid" | "list"
}

class Address {
  street string
  city string
  state string
  country string
  postalCode string
  coordinates Coordinates?
}

class Coordinates {
  latitude float
  longitude float
}

class Metadata {
  createdAt string
  updatedAt string
  version int
  tags string[]
  attributes map<string, string>
}

class DeeplyNested {
  level1 Level1
}

class Level1 {
  data string
  level2 Level2
}

class Level2 {
  data string
  level3 Level3
}

class Level3 {
  data string
  level4 Level4
}

class Level4 {
  data string
  level5 Level5
}

class Level5 {
  data string
  items string[]
  mapping map<string, int>
}

class ComplexNested {
  company Company
  employees Employee[]
  projects Project[]
}

class Company {
  id int
  name string
  address Address
  departments Department[]
  metadata CompanyMetadata
}

class Department {
  id int
  name string
  manager Employee?
  members Employee[]
  budget float
  projects Project[]
}

class Employee {
  id int
  name string
  email string
  role string
  department string
  skills string[]
  address Address?
  emergencyContact Contact?
}

class Contact {
  name string
  relationship string
  phone string
  email string?
}

class Project {
  id int
  name string
  description string
  status "planning" | "active" | "completed" | "cancelled"
  team Employee[]
  milestones Milestone[]
  budget Budget
}

class Milestone {
  id int
  name string
  dueDate string
  completed bool
  tasks Task[]
}

class Task {
  id int
  title string
  description string
  assignee string
  priority "low" | "medium" | "high"
  status "todo" | "in_progress" | "done"
  subtasks Task[]?
}

class Budget {
  total float
  spent float
  categories map<string, float>
  approvals Approval[]
}

class Approval {
  approver string
  date string
  amount float
  notes string?
}

class CompanyMetadata {
  founded string
  industry string
  size "small" | "medium" | "large" | "enterprise"
  certifications string[]
  partnerships Company[]?
}

class RecursiveStructure {
  id int
  name string
  children RecursiveStructure[]
  parent RecursiveStructure?
  metadata map<string, string | int | bool>
}

function TestSimpleNested(input: string) -> SimpleNested {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a SimpleNested object with realistic data for:
    - A user with complete profile and settings
    - A full address with coordinates
    - Metadata with various attributes
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestDeeplyNested(input: string) -> DeeplyNested {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a DeeplyNested object with data at all 5 levels.
    Each level should have meaningful data.
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestComplexNested(input: string) -> ComplexNested {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a ComplexNested object representing a small tech company with:
    - Company details with 2 departments
    - 5 employees across departments
    - 2 active projects with milestones and tasks
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}

function TestRecursiveStructure(input: string) -> RecursiveStructure {
  client "openai/gpt-4o-mini"
  prompt #"
    Return a RecursiveStructure representing a tree with:
    - Root node
    - At least 2 child nodes
    - Some children have their own children (3 levels deep)
    - Include parent references where applicable
    
    {{ ctx.output_format }}
    
    Input: {{ input }}
  "#
}