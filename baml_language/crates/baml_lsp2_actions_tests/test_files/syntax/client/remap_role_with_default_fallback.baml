client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    allowed_roles ["user", "system"]
    default_role "system"
    remap_roles {
      system "instructions"
    }
  }
}

function TestRemapWithFallback(input: string) -> string {
  client MyClient
  prompt #"
    {{ _.chat("assistant") }}
    This should fallback to system role and then be remapped to instructions: {{ input }}
  "#
}

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient = openai.OpenAiClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//     ╭─[client_remap_role_with_default_fallback.baml:1:1]
//   1 │ ╭─▶ client<llm> MyClient {
//   2 │ │     provider openai
//   3 │ │     options {
//   4 │ │       model "gpt-4o"
//   5 │ │       allowed_roles ["user", "system"]
//   6 │ │       default_role "system"
//   7 │ │       remap_roles {
//   8 │ │         system "instructions"
//   9 │ │       }
//  10 │ │     }
//  11 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
// E0010
//
//   × LLM function `TestRemapWithFallback` uses a Jinja `#"..."#` prompt, which is removed; write a backtick template: prompt `... ${ctx.output_format}`
//     ╭─[client_remap_role_with_default_fallback.baml:13:57]
//  13 │ ╭─▶ function TestRemapWithFallback(input: string) -> string {
//  14 │ │     client MyClient
//  15 │ │     prompt #"
//  16 │ │       {{ _.chat("assistant") }}
//  17 │ │       This should fallback to system role and then be remapped to instructions: {{ input }}
//  18 │ │     "#
//  19 │ ├─▶ }
//     · ╰──── migrate to a backtick prompt
//     ╰────
