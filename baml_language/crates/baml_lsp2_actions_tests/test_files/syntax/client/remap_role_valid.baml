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
  client: MyClient
  prompt: `
    ${role("user")}
    Test message: ${input}
  `
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
// E0003
//
//   × unresolved name: `MyClient`
//     ╭─[client_remap_role_valid.baml:16:11]
//  16 │   client: MyClient
//     ·           ────────
//     ╰────
