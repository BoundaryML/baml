client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    allowed_roles ["user", "assistant", "system"]
    remap_roles {
      user "human"
      assistant "ai"
      system "instructions"
    }
    default_role "user"
  }
}

function TestRemap(input: string) -> string {
  client MyClient
  prompt #"
    {{ _.chat("user") }}
    Test message: {{ input }}
  "#
}

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient = openai.OpenAiClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//     ╭─[client_remap_role_valid.baml:1:1]
//   1 │ ╭─▶ client<llm> MyClient {
//   2 │ │     provider openai
//   3 │ │     options {
//   4 │ │       model "gpt-4o"
//   5 │ │       allowed_roles ["user", "assistant", "system"]
//   6 │ │       remap_roles {
//   7 │ │         user "human"
//   8 │ │         assistant "ai"
//   9 │ │         system "instructions"
//  10 │ │       }
//  11 │ │       default_role "user"
//  12 │ │     }
//  13 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
// E0010
//
//   × LLM function `TestRemap` uses a Jinja `#"..."#` prompt, which is removed; write a backtick template: prompt `... ${ctx.output_format}`
//     ╭─[client_remap_role_valid.baml:15:45]
//  15 │ ╭─▶ function TestRemap(input: string) -> string {
//  16 │ │     client MyClient
//  17 │ │     prompt #"
//  18 │ │       {{ _.chat("user") }}
//  19 │ │       Test message: {{ input }}
//  20 │ │     "#
//  21 │ ├─▶ }
//     · ╰──── migrate to a backtick prompt
//     ╰────
