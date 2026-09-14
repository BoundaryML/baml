// BAML golden sample: one file that exercises the language surface used to
// validate every highlighter port (TextMate, Sublime, highlight.js, KDE,
// tree-sitter) and shipped as the Shiki registry sample. Every construct
// below is real syntax taken from the compiler test suite — extend it when
// the language grows, and keep it under 100 columns.

class Resume {
  name string @description("Candidate name") @alias("full_name")
  email string?
  skills string[]
  scores map<string, int | float>
  level: Level
  @@dynamic
}

enum Level {
  Junior
  Senior @description("Five or more years")
  @@description("Seniority ladder")
}

interface Scored<T, E> {
  function score(self, value: T) -> float throws E
}

client<llm> GPT4 {
  provider openai
  retry_policy SafeRetry
  options {
    model "gpt-4o"
    api_key env.OPENAI_API_KEY
  }
}

retry_policy SafeRetry {
  max_retries 3
}

generator ts_target {
  output_type: "typescript/node",
  output_dir: "typescript_demo",
}

template_string Persona(role: string) #"
  You are a {{ role }} reviewing resumes.
"#

function ExtractResume(text: string, hints: string[]) -> Resume {
  client GPT4
  prompt #"
    {{ Persona("recruiter") }}
    {% for hint in hints %}
    - Focus on {{ hint }}
    {% endfor %}
    Extract a structured resume from: {{ text }}
  "#
}

function Tally(v: int | string, items: int[]) -> int {
  const base: int = 16;
  const big = 9007199254740993n;
  const rate = 1.5e-3;
  let total: int = v else { return base; };
  for (let item in items) {
    total += item;
  };
  while (total < 100) {
    total = total * 2;
  };
  defer {
    total;
  }
  match (v) {
    let n: int => n + total,
    string => total - 1,
    _ => 0
  }
}

function Describe(flag: bool) -> string {
  let raw = ##"raw #"quoted"# text"##;
  let ticked = `backtick with \` escape`;
  if (flag) {
    raw
  } else {
    ticked
  }
}

test "GoldenExtract" {
  ExtractResume(
    /* who */ "Jane Doe, 12 years of Rust",
    ["skills", "email"],
  )
}
