// examples of code the LLM may generate that we need to fix

use super::*;

fn code_class() -> TyResolved<'static, &'static str> {
    class_ty(
        "Test",
        vec![
            field("type", literal_string("code")),
            field("code", string_ty()),
        ],
    )
}

fn code_db() -> TypeRefDb<'static, &'static str> {
    empty_db()
}

test_deserializer!(
    backticks,
    r#"
    {
      "type": "code",
      "code": `print("Hello, world!")`
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print(\"Hello, world!\")"
    }
);

test_deserializer!(
    single_quotes,
    r#"
    {
      "type": "code",
      "code": 'print("Hello, world!")'
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print(\"Hello, world!\")"
    }
);

test_deserializer!(
    double_quotes,
    r#"
    {
      "type": "code",
      "code": "print(\"Hello, world!\")"
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print(\"Hello, world!\")"
    }
);

test_deserializer!(
    unquoted_string,
    r#"
    {
      "type": "code",
      "code": "print(\"Hello, world!\")"
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print(\"Hello, world!\")"
    }
);

test_deserializer!(
    triple_quotes,
    r#"
    {
      "type": "code",
      "code": """print("Hello, world!")"""
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print(\"Hello, world!\")"
    }
);

test_deserializer!(
    triple_quotes_contains_only_quoted_string,
    r#"
    {
      "code": """
"Hello, world!"
"""
      "type": "code",
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "\"Hello, world!\""
    }
);

test_deserializer!(
    triple_quotes_contains_only_quoted_string_dedent,
    r#"
  {
    "code": """
        def main():
          print("Hello, world!")
    """,
    "type": "code",
  }
  "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "def main():\n  print(\"Hello, world!\")"
    }
);

test_deserializer!(
    unescaped_newline_double_quotes,
    r#"
    {
      "type": "code",
      "code": "print(\"Hello, world!
Goodbye, world!\")"
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print(\"Hello, world!\nGoodbye, world!\")"
    }
);

test_deserializer!(
    unescaped_newline_backticks,
    r#"
    {
      "type": "code",
      "code": `print("Hello, world!
Goodbye, world!")`
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print(\"Hello, world!\nGoodbye, world!\")"
    }
);

test_deserializer!(
    unescaped_newline_single_quotes,
    r#"
    {
      "type": "code",
      "code": 'print("Hello, world!
Goodbye, world!")'
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print(\"Hello, world!\nGoodbye, world!\")"
    }
);

test_deserializer!(
    unescaped_newline_triple_quotes,
    r#"
    {
      "type": "code",
      "code": """print("Hello, world!
Goodbye, world!")"""
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print(\"Hello, world!\nGoodbye, world!\")"
    }
);

test_deserializer!(
    unescaped_double_quotes_in_double_quotes,
    r#"
    {
      "type": "code",
      "code": "print("Hello, world!")"
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print(\"Hello, world!\")"
    }
);

test_deserializer!(
    unescaped_double_quotes_in_backticks,
    r#"
    {
      "type": "code",
      "code": `print("Hello, world!")`
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print(\"Hello, world!\")"
    }
);

test_deserializer!(
    unescaped_single_quotes_in_single_quotes,
    r#"
    {
      "type": "code",
      "code": 'print('Hello, world!')'
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print('Hello, world!')"
    }
);

test_deserializer!(
    unescaped_double_quotes_in_triple_quotes,
    r#"
    {
      "type": "code",
      "code": """print("Hello, world!")"""
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print(\"Hello, world!\")"
    }
);

// TODO: THIS PARSES INCORRECTLY! Rare case, but should be fixed
// if a customer complains about it.
// https://github.com/BoundaryML/baml/issues/1145
test_deserializer!(
    unescaped_single_quotes_in_triple_quotes,
    r#"
    {
      "type": "code",
      "code": """print("""Hello, world!""")"""
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "print("
    }
);

test_deserializer!(
    unescaped_backticks_in_backticks,
    r#"
    {
      "type": "code",
      "code": `console.log(`Hello, world!`)`
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "console.log(`Hello, world!`)"
    }
);

test_deserializer!(
    large_backticks,
    r#"
  {
    "type": "code",
  "code": `import { query } from './_generated/server';
import { v } from 'convex/values';

export default query(async (ctx) => {
  const posts = await ctx.db
    .query('posts')
    .order('desc')
    .collect();

  const postsWithDetails = await Promise.all(
    posts.map(async (post) => {
      // Fetch author information
      const author = await ctx.db.get(post.authorId);
      if (!author) {
        throw new Error('Author not found');
      }

      // Count upvotes
      const upvotes = await ctx.db
        .query('upvotes')
        .filter((q) => q.eq(q.field('postId'), post._id))
        .collect();

      return {
        id: post._id.toString(),
        title: post.title,
        content: post.content,
        author: {
          id: author._id.toString(),
          name: author.name,
        },
        upvoteCount: upvotes.length,
        createdAt: post._creationTime.toString(),
      };
    })
  );

  return postsWithDetails;
})`
  }
  "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": r#"import { query } from './_generated/server';
import { v } from 'convex/values';

export default query(async (ctx) => {
  const posts = await ctx.db
    .query('posts')
    .order('desc')
    .collect();

  const postsWithDetails = await Promise.all(
    posts.map(async (post) => {
      // Fetch author information
      const author = await ctx.db.get(post.authorId);
      if (!author) {
        throw new Error('Author not found');
      }

      // Count upvotes
      const upvotes = await ctx.db
        .query('upvotes')
        .filter((q) => q.eq(q.field('postId'), post._id))
        .collect();

      return {
        id: post._id.toString(),
        title: post.title,
        content: post.content,
        author: {
          id: author._id.toString(),
          name: author.name,
        },
        upvoteCount: upvotes.length,
        createdAt: post._creationTime.toString(),
      };
    })
  );

  return postsWithDetails;
})"#
    }
);

test_deserializer!(
    triple_backticks,
    r#"
Here's a comparison of TypeScript and Ruby code for checking the main Git branch using subprocesses:

{
  "code": ```
const { execSync } = require('child_process');

function getMainBranch(): string {
  try {
    // Try 'main' first
    const mainExists = execSync('git rev-parse --verify main 2>/dev/null', { encoding: 'utf8' });
    if (mainExists) return 'main';
  } catch {
    // Try 'master' if 'main' doesn't exist
    try {
      const masterExists = execSync('git rev-parse --verify master 2>/dev/null', { encoding: 'utf8' });
      if (masterExists) return 'master';
    } catch {
      throw new Error('Neither main nor master branch found');
    }
  }

  throw new Error('Unable to determine main branch');
}

// Usage
try {
  const mainBranch = getMainBranch();
  console.log(`Main branch is: ${mainBranch}`);
} catch (error) {
  console.error(`Error: ${error.message}`);
}
```,
    "type": "code",
}

Both versions will:
1. First check if 'main' exists
2. If not, check if 'master' exists
3. Return the appropriate branch name
4. Throw/raise an error if neither exists
5. Handle errors gracefully

The main difference is that Ruby uses the special `$?` variable to check command success, while TypeScript relies on try/catch with execSync.

  "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": r#"const { execSync } = require('child_process');

function getMainBranch(): string {
  try {
    // Try 'main' first
    const mainExists = execSync('git rev-parse --verify main 2>/dev/null', { encoding: 'utf8' });
    if (mainExists) return 'main';
  } catch {
    // Try 'master' if 'main' doesn't exist
    try {
      const masterExists = execSync('git rev-parse --verify master 2>/dev/null', { encoding: 'utf8' });
      if (masterExists) return 'master';
    } catch {
      throw new Error('Neither main nor master branch found');
    }
  }

  throw new Error('Unable to determine main branch');
}

// Usage
try {
  const mainBranch = getMainBranch();
  console.log(`Main branch is: ${mainBranch}`);
} catch (error) {
  console.error(`Error: ${error.message}`);
}"#
    }
);

test_deserializer!(
    triple_quotes_contains_only_backtick_string,
    r#"
    {
      "code": ```
`Hello, world!`
```,
      "type": "code",
    }
    "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "`Hello, world!`"
    }
);

test_deserializer!(
    triple_backticks_returns_dedented_code_and_discards_info,
    r#"
Here's a comparison of TypeScript and Ruby code for checking the main Git branch using subprocesses:

{
  "code": ```typescript main.ts
    const async function main() {
      console.log("Hello, world!");
    }
```,
    "type": "code",
}

  "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": r#"const async function main() {
  console.log("Hello, world!");
}"#
    }
);

test_deserializer!(
    triple_backticks_contains_json_terminators,
    r#"
Here's a comparison of TypeScript and Ruby code for checking the main Git branch using subprocesses:

{
  "code": ```
  { type: "code", code: "aaa", closing_terminators: }}}]])) }
```,
    "type": "code",
}

  "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": r#"{ type: "code", code: "aaa", closing_terminators: }}}]])) }"#
    }
);

test_deserializer!(
    triple_backticks_in_json_fenced_codeblock,
    r#"
Here's a comparison of TypeScript and Ruby code for checking the main Git branch using subprocesses:

```json
{
  "code": ```
  { type: "code", code: "aaa", closing_terminators: }}}]])) }
```,
    "type": "code",
}
```

  "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": r#"{ type: "code", code: "aaa", closing_terminators: }}}]])) }"#
    }
);

test_deserializer!(
    string_preserves_triple_backticks,
    r#"
Here's a comparison of TypeScript and Ruby code for checking the main Git branch using subprocesses:

```json
{
  "code": "```
const { execSync } = require('child_process');
```",
    "type": "code",
}
```

  "#,
    code_class(),
    code_db(),
    {
      "type": "code",
      "code": "```\nconst { execSync } = require('child_process');\n```"
    }
);
