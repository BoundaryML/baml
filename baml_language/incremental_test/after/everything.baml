// ============================================================================
// COMPREHENSIVE BAML EXAMPLE - All Features Showcase
// ============================================================================
// This file demonstrates every major BAML feature including:
// - Classes with various types, circular references, and type aliases
// - Checks and assertions on fields and classes
// - Multiple client types (llm, fallback, round-robin) with retry policies
// - LLM functions with prompts including loops
// - Expression functions with control flow, loops, and baml.fetch_as
// ============================================================================

// ============================================================================
// TYPE ALIASES
// ============================================================================

type UserId = string
type Score = float
type Tags = string[]

// ============================================================================
// CLASSES - Various Types and Circular References
// ============================================================================

/// A user profile with various field types
class User {
  id UserId @check({{ this|length > 0 }})
  name string @check(non_empty, {{ this|length > 0 }})
  email string @check(valid_email, {{ this|regex_match("^[^@]+@[^@]+\\.[^@]+$") }})
  age int @check(valid_age, {{ this >= 0 and this <= 150 }})
  score Score @check({{ this >= 0.0 and this <= 100.0 }})
  tags Tags
  is_active bool
  profile UserProfile?
  friends User[] // Circular reference!
  metadata Metadata

  @@assert(age_score_check, {{ this.age > 18 or this.score < 50.0 }})
}

/// Nested profile information
class UserProfile {
  bio string @check({{ this|length < 500 }})
  avatar_url string?
  created_at string
  settings ProfileSettings
  owner User? // Circular reference back to User
}

class ProfileSettings {
  theme string @check(theme, {{ this in ["light", "dark", "auto"] }})
  language string @check(lang, {{ this|length == 2 }})
  notifications bool
}

class Metadata {
  created_by string
  updated_by string
  version int @check(version, {{ this > 0 }})

  @@check(version_bounds, {{ this.version >= 1 and this.version <= 1000 }})
}

/// A category enum for classification
enum Category {
  Technology
  Science
  Arts
  Sports
  Entertainment
}

/// Article with references to other types
class Article {
  title string @check({{ this|length > 0 and this|length <= 200 }})
  content string @check({{ this|length > 10 }})
  author User
  category Category
  related_articles Article[] // Circular reference
  word_count int @check({{ this > 0 }})

  @@assert(content_length, {{ this.word_count <= (this.content|length) }})
}

// ============================================================================
// RETRY POLICIES
// ============================================================================

retry_policy ExponentialBackoff {
  max_retries 5
  strategy {
    type exponential_backoff
  }
}

retry_policy ConstantDelay {
  max_retries 3
  strategy {
    type constant_delay
    delay_ms 200
  }
}

// ============================================================================
// LLM CLIENTS
// ============================================================================

client<llm> GPT4o {
  provider openai
  retry_policy ExponentialBackoff
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
    max_tokens 2000
  }
}

client<llm> GPT4oMini {
  provider openai
  options {
    model gpt-4o-mini
    api_key env.OPENAI_API_KEY
    max_tokens 1000
  }
}

client<llm> Claude {
  provider anthropic
  retry_policy ConstantDelay
  options {
    model claude-3-5-sonnet-20241022
    api_key env.ANTHROPIC_API_KEY
    max_tokens 2000
  }
}

client<llm> ClaudeHaiku {
  provider anthropic
  options {
    model claude-3-haiku-20240307
    api_key env.ANTHROPIC_API_KEY
    max_tokens 1000
  }
}

// ============================================================================
// COMPOUND CLIENTS - Fallback and Round-Robin
// ============================================================================

/// Fallback client: tries GPT4o first, then Claude, then GPT4oMini
client<llm> ResilientClient {
  provider baml-fallback
  retry_policy ExponentialBackoff
  options {
    strategy [
      GPT4o
      Claude
      GPT4oMini
    ]
  }
}

/// Round-robin client: alternates between models
client<llm> LoadBalancedClient {
  provider baml-round-robin
  options {
    start 0
    strategy [
      ClaudeHaiku
      GPT4oMini
    ]
  }
}

// ============================================================================
// LLM FUNCTIONS WITH PROMPTS
// ============================================================================

/// Generate a user profile using loops in the prompt
function GenerateUserProfile(
  user_ids: UserId[],
  category_filter: Category
) -> User {
  client ResilientClient
  prompt #"
    {{ _.role("system") }}
    You are a helpful assistant that generates realistic user profiles.

    {{ _.role("user") }}
    Generate a complete user profile with the following requirements:

    Reference User IDs to avoid:
    {% for uid in user_ids %}
    - {{ uid }}
    {% endfor %}

    Category preference: {{ category_filter }}

    {{ ctx.output_format }}
  "#
}

/// Summarize multiple articles with for loop
function SummarizeArticles(articles: Article[]) -> string {
  client GPT4o
  prompt #"
    {{ _.role("system") }}
    You are an expert at summarizing articles.

    {{ _.role("user") }}
    Summarize the following articles:

    {% for article in articles %}
    ## Article {{ loop.index }}: {{ article.title }}
    Author: {{ article.author.name }}
    Category: {{ article.category }}
    Content: {{ article.content }}
    Word Count: {{ article.word_count }}

    {% endfor %}

    Provide a comprehensive summary of all articles.
  "#
}

/// Classify content with conditional prompts
function ClassifyContent(content: string, user_tags: Tags) -> Category {
  client LoadBalancedClient
  prompt #"
    {{ _.role("system") }}
    You are a content classification expert.

    {{ _.role("user") }}
    Classify the following content into one of these categories:
    Technology, Science, Arts, Sports, Entertainment

    Content: {{ content }}

    {% if user_tags|length > 0 %}
    User's interests:
    {% for tag in user_tags %}
    - {{ tag }}
    {% endfor %}
    {% endif %}

    {{ ctx.output_format }}
  "#
}

/// Generate related articles based on existing ones
function GenerateRelatedArticles(
  base_article: Article,
  count: int
) -> Article[] {
  client Claude
  prompt #"
    {{ _.role("system") }}
    You are a content generator that creates related articles.

    {{ _.role("user") }}
    Generate {{ count }} related articles based on:

    Title: {{ base_article.title }}
    Category: {{ base_article.category }}
    Content preview: {{ base_article.content[:200] }}

    {% if base_article.related_articles|length > 0 %}
    Existing related articles to differentiate from:
    {% for related in base_article.related_articles %}
    - {{ related.title }}
    {% endfor %}
    {% endif %}

    {{ ctx.output_format }}
  "#
}

// ============================================================================
// EXPRESSION FUNCTIONS
// ============================================================================

/// Simple expression function with regular variables
function CalculateScore(base: int, multiplier: float) -> Score {
  let adjusted = base * multiplier;
  let clamped = if (adjusted > 100.0) { 100.0 } else { adjusted };
  clamped
}

/// Expression function with watch variables for monitoring
function ProcessUserData(user: User) -> int {
  watch let current_step: string = "initialization";
  current_step = "validating";

  watch let score: Score = user.score;

  // Regular variable
  let adjusted_score = if (user.is_active) {
    current_step = "applying_active_bonus";
    score * 1.2
  } else {
    current_step = "no_bonus";
    score
  };

  current_step = "completed";

  // Return as integer
  adjusted_score
}

/// Expression function with for-in loop over list
function CountActiveFriends(user: User) -> int {
  let count = 0;

  for (let friend in user.friends) {
    if (friend.is_active) {
      count += 1;
    }
  }

  count
}

/// Expression function with c-style for loop
function GenerateScoreSequence(start: int, end: int, step: int) -> int[] {
  let scores: int[] = [];

  for (let i = start; i <= end; i += step) {
    let score = i * 2;
    scores = scores + [score];
  }

  scores
}

/// Expression function with while loop
function ComputeFibonacci(n: int) -> int {
  let a = 0;
  let b = 1;

  if (n == 0) {
    a
  } else {
    let i = 1;
    while (i <= n) {
      let temp = a + b;
      a = b;
      b = temp;
      i += 1;
    }
    a
  }
}

/// Expression function with if statements (not expressions)
function CategorizeUser(user: User) -> string {
  let category = "unknown";

  if (user.age < 18) {
    category = "minor";
  } else if (user.age < 65) {
    category = "adult";
  } else {
    category = "senior";
  }

  if (user.score > 80.0) {
    category = category + "_high_performer";
  }

  category
}

/// Expression function with nested if expressions
function DetermineAccessLevel(user: User) -> int {
  let base_level = if (user.is_active) {
    if (user.age >= 18) { 3 } else { 2 }
  } else {
    1
  };

  let bonus = if (user.score > 90.0) { 1 } else { 0 };

  base_level + bonus
}

/// Expression function using baml.fetch_as for external API
class ExternalUserData {
  id int
  name string
  email string
}

function FetchExternalUser(user_id: int) -> ExternalUserData {
  let url = "https://jsonplaceholder.typicode.com/users/" + user_id;
  let user_data = baml.fetch_as<ExternalUserData>(url);
  user_data
}

/// Expression function with HTTP POST using baml.fetch_as
class TodoItem {
  id int
  todo string
  completed bool
  userId int
}

function CreateTodo(task: string, user_id: int) -> TodoItem {
  baml.fetch_as<TodoItem>(baml.HttpRequest {
    method: baml.HttpMethod.Post,
    url: "https://dummyjson.com/todos/add",
    json: {
      "todo": task,
      "completed": false,
      "userId": user_id
    }
  })
}

/// Complex expression function combining LLM calls and control flow
function EnrichUserProfile(user: User, fetch_external: bool) -> User {
  watch let status: string = "starting";
  status = "processing";

  let enriched_user = user;

  // Fetch external data if requested
  if (fetch_external) {
    status = "fetching_external";
    let external = FetchExternalUser(1);
    // In a real scenario, we'd merge the data
  }

  // Adjust score based on friend count
  let friend_count = CountActiveFriends(user);

  if (friend_count > 5) {
    status = "applying_social_bonus";
    enriched_user.score = CalculateScore(enriched_user.score, 1.1);
  }

  status = "completed";
  enriched_user
}

/// Expression function with watch variables and filter
function ProcessTagsWithFilter(tags: Tags) -> int {
  watch let current_tag: string = "";
  current_tag.$watch.options(baml.WatchOptions {
    when: IsLongTag
  })

  let count = 0;
  for (let tag in tags) {
    current_tag = tag;
    count += 1;
  }

  count
}

/// Filter function for watch variables
function IsLongTag(tag: string) -> bool {
  tag|length > 10
}

// ============================================================================
// TESTS
// ============================================================================

test TestGenerateUserProfile {
  functions [GenerateUserProfile]
  args {
    user_ids ["user_001", "user_002"]
    category_filter Category.Technology
  }
  @@assert({{ this.name|length > 0 }})
  @@assert({{ this.email|regex_match("^[^@]+@[^@]+\\.[^@]+$") }})
}

test TestCalculateScore {
  functions [CalculateScore]
  args {
    base 85
    multiplier 1.1
  }
  @@assert({{ this <= 100.0 }})
}

test TestComputeFibonacci {
  functions [ComputeFibonacci]
  args {
    n 10
  }
  @@assert({{ this == 55 }})
}

test TestGenerateScoreSequence {
  functions [GenerateScoreSequence]
  args {
    start 1
    end 10
    step 2
  }
  @@assert({{ this|length == 5 }})
}
