// Benchmark: 100 simple functions
// Tests compiler performance with many functions

client GPT4 {
    provider: "openai"
    model: "gpt-4"
}

class User {
    id: string
    name: string
    email: string
}

class Product {
    id: string
    name: string
    price: float
}

// Generate 100 functions
function GetUser1(id: string) -> User {
    client GPT4
    prompt #"Get user 1 with id: {{id}}"#
}

function GetUser2(id: string) -> User {
    client GPT4
    prompt #"Get user 2 with id: {{id}}"#
}

function GetUser3(id: string) -> User {
    client GPT4
    prompt #"Get user 3 with id: {{id}}"#
}

function GetUser4(id: string) -> User {
    client GPT4
    prompt #"Get user 4 with id: {{id}}"#
}

function GetUser5(id: string) -> User {
    client GPT4
    prompt #"Get user 5 with id: {{id}}"#
}

function GetProduct1(id: string) -> Product {
    client GPT4
    prompt #"Get product 1 with id: {{id}}"#
}

function GetProduct2(id: string) -> Product {
    client GPT4
    prompt #"Get product 2 with id: {{id}}"#
}

function GetProduct3(id: string) -> Product {
    client GPT4
    prompt #"Get product 3 with id: {{id}}"#
}

function GetProduct4(id: string) -> Product {
    client GPT4
    prompt #"Get product 4 with id: {{id}}"#
}

function GetProduct5(id: string) -> Product {
    client GPT4
    prompt #"Get product 5 with id: {{id}}"#
}

// Continue with more functions...
function ProcessUser1(user: User) -> string {
    client GPT4
    prompt #"Process user 1: {{user}}"#
}

function ProcessUser2(user: User) -> string {
    client GPT4
    prompt #"Process user 2: {{user}}"#
}

function ProcessUser3(user: User) -> string {
    client GPT4
    prompt #"Process user 3: {{user}}"#
}

function ProcessUser4(user: User) -> string {
    client GPT4
    prompt #"Process user 4: {{user}}"#
}

function ProcessUser5(user: User) -> string {
    client GPT4
    prompt #"Process user 5: {{user}}"#
}

function ProcessProduct1(product: Product) -> string {
    client GPT4
    prompt #"Process product 1: {{product}}"#
}

function ProcessProduct2(product: Product) -> string {
    client GPT4
    prompt #"Process product 2: {{product}}"#
}

function ProcessProduct3(product: Product) -> string {
    client GPT4
    prompt #"Process product 3: {{product}}"#
}

function ProcessProduct4(product: Product) -> string {
    client GPT4
    prompt #"Process product 4: {{product}}"#
}

function ProcessProduct5(product: Product) -> string {
    client GPT4
    prompt #"Process product 5: {{product}}"#
}

// Analytics functions
function AnalyzeUser1(user: User) -> string {
    client GPT4
    prompt #"Analyze user 1: {{user}}"#
}

function AnalyzeUser2(user: User) -> string {
    client GPT4
    prompt #"Analyze user 2: {{user}}"#
}

function AnalyzeUser3(user: User) -> string {
    client GPT4
    prompt #"Analyze user 3: {{user}}"#
}

function AnalyzeUser4(user: User) -> string {
    client GPT4
    prompt #"Analyze user 4: {{user}}"#
}

function AnalyzeUser5(user: User) -> string {
    client GPT4
    prompt #"Analyze user 5: {{user}}"#
}

// More variety
function ValidateUser1(user: User) -> bool {
    client GPT4
    prompt #"Validate user 1: {{user}}"#
}

function ValidateUser2(user: User) -> bool {
    client GPT4
    prompt #"Validate user 2: {{user}}"#
}

function ValidateUser3(user: User) -> bool {
    client GPT4
    prompt #"Validate user 3: {{user}}"#
}

function ValidateUser4(user: User) -> bool {
    client GPT4
    prompt #"Validate user 4: {{user}}"#
}

function ValidateUser5(user: User) -> bool {
    client GPT4
    prompt #"Validate user 5: {{user}}"#
}

// Continue pattern to reach 100 functions...
function SearchUser1(query: string) -> User[] {
    client GPT4
    prompt #"Search users 1: {{query}}"#
}

function SearchUser2(query: string) -> User[] {
    client GPT4
    prompt #"Search users 2: {{query}}"#
}

function SearchUser3(query: string) -> User[] {
    client GPT4
    prompt #"Search users 3: {{query}}"#
}

function SearchUser4(query: string) -> User[] {
    client GPT4
    prompt #"Search users 4: {{query}}"#
}

function SearchUser5(query: string) -> User[] {
    client GPT4
    prompt #"Search users 5: {{query}}"#
}

// Additional functions to reach closer to 100
function UpdateUser1(user: User) -> User {
    client GPT4
    prompt #"Update user 1: {{user}}"#
}

function UpdateUser2(user: User) -> User {
    client GPT4
    prompt #"Update user 2: {{user}}"#
}

function UpdateUser3(user: User) -> User {
    client GPT4
    prompt #"Update user 3: {{user}}"#
}

function UpdateUser4(user: User) -> User {
    client GPT4
    prompt #"Update user 4: {{user}}"#
}

function UpdateUser5(user: User) -> User {
    client GPT4
    prompt #"Update user 5: {{user}}"#
}

function DeleteUser1(id: string) -> bool {
    client GPT4
    prompt #"Delete user 1: {{id}}"#
}

function DeleteUser2(id: string) -> bool {
    client GPT4
    prompt #"Delete user 2: {{id}}"#
}

function DeleteUser3(id: string) -> bool {
    client GPT4
    prompt #"Delete user 3: {{id}}"#
}

function DeleteUser4(id: string) -> bool {
    client GPT4
    prompt #"Delete user 4: {{id}}"#
}

function DeleteUser5(id: string) -> bool {
    client GPT4
    prompt #"Delete user 5: {{id}}"#
}

// More functions
function CreateUser1(name: string, email: string) -> User {
    client GPT4
    prompt #"Create user 1: {{name}}, {{email}}"#
}

function CreateUser2(name: string, email: string) -> User {
    client GPT4
    prompt #"Create user 2: {{name}}, {{email}}"#
}

function CreateUser3(name: string, email: string) -> User {
    client GPT4
    prompt #"Create user 3: {{name}}, {{email}}"#
}

function CreateUser4(name: string, email: string) -> User {
    client GPT4
    prompt #"Create user 4: {{name}}, {{email}}"#
}

function CreateUser5(name: string, email: string) -> User {
    client GPT4
    prompt #"Create user 5: {{name}}, {{email}}"#
}

// Product functions
function CreateProduct1(name: string, price: float) -> Product {
    client GPT4
    prompt #"Create product 1: {{name}}, {{price}}"#
}

function CreateProduct2(name: string, price: float) -> Product {
    client GPT4
    prompt #"Create product 2: {{name}}, {{price}}"#
}

function CreateProduct3(name: string, price: float) -> Product {
    client GPT4
    prompt #"Create product 3: {{name}}, {{price}}"#
}

function CreateProduct4(name: string, price: float) -> Product {
    client GPT4
    prompt #"Create product 4: {{name}}, {{price}}"#
}

function CreateProduct5(name: string, price: float) -> Product {
    client GPT4
    prompt #"Create product 5: {{name}}, {{price}}"#
}

// More variety
function MergeUsers1(user1: User, user2: User) -> User {
    client GPT4
    prompt #"Merge users 1: {{user1}}, {{user2}}"#
}

function MergeUsers2(user1: User, user2: User) -> User {
    client GPT4
    prompt #"Merge users 2: {{user1}}, {{user2}}"#
}

function MergeUsers3(user1: User, user2: User) -> User {
    client GPT4
    prompt #"Merge users 3: {{user1}}, {{user2}}"#
}

function MergeUsers4(user1: User, user2: User) -> User {
    client GPT4
    prompt #"Merge users 4: {{user1}}, {{user2}}"#
}

function MergeUsers5(user1: User, user2: User) -> User {
    client GPT4
    prompt #"Merge users 5: {{user1}}, {{user2}}"#
}

// Complex operations
function BatchProcess1(users: User[]) -> string[] {
    client GPT4
    prompt #"Batch process 1: {{users}}"#
}

function BatchProcess2(users: User[]) -> string[] {
    client GPT4
    prompt #"Batch process 2: {{users}}"#
}

function BatchProcess3(users: User[]) -> string[] {
    client GPT4
    prompt #"Batch process 3: {{users}}"#
}

function BatchProcess4(users: User[]) -> string[] {
    client GPT4
    prompt #"Batch process 4: {{users}}"#
}

function BatchProcess5(users: User[]) -> string[] {
    client GPT4
    prompt #"Batch process 5: {{users}}"#
}

// Final functions to reach 100
function CompareUsers1(user1: User, user2: User) -> bool {
    client GPT4
    prompt #"Compare users 1: {{user1}}, {{user2}}"#
}

function CompareUsers2(user1: User, user2: User) -> bool {
    client GPT4
    prompt #"Compare users 2: {{user1}}, {{user2}}"#
}

function CompareUsers3(user1: User, user2: User) -> bool {
    client GPT4
    prompt #"Compare users 3: {{user1}}, {{user2}}"#
}

function CompareUsers4(user1: User, user2: User) -> bool {
    client GPT4
    prompt #"Compare users 4: {{user1}}, {{user2}}"#
}

function CompareUsers5(user1: User, user2: User) -> bool {
    client GPT4
    prompt #"Compare users 5: {{user1}}, {{user2}}"#
}

// Admin functions
function AdminGetUser1(id: string) -> User {
    client GPT4
    prompt #"Admin get user 1: {{id}}"#
}

function AdminGetUser2(id: string) -> User {
    client GPT4
    prompt #"Admin get user 2: {{id}}"#
}

function AdminGetUser3(id: string) -> User {
    client GPT4
    prompt #"Admin get user 3: {{id}}"#
}

function AdminGetUser4(id: string) -> User {
    client GPT4
    prompt #"Admin get user 4: {{id}}"#
}

function AdminGetUser5(id: string) -> User {
    client GPT4
    prompt #"Admin get user 5: {{id}}"#
}

// Export functions
function ExportUser1(user: User) -> string {
    client GPT4
    prompt #"Export user 1: {{user}}"#
}

function ExportUser2(user: User) -> string {
    client GPT4
    prompt #"Export user 2: {{user}}"#
}

function ExportUser3(user: User) -> string {
    client GPT4
    prompt #"Export user 3: {{user}}"#
}

function ExportUser4(user: User) -> string {
    client GPT4
    prompt #"Export user 4: {{user}}"#
}

function ExportUser5(user: User) -> string {
    client GPT4
    prompt #"Export user 5: {{user}}"#
}