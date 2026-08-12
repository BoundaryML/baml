client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    allowed_roles ["user", "assistant"]
    remap_roles {
      user "human"
      system "instructions"
    }
  }
}

// error: remap_roles values must be one of: Value("user"), Value("assistant"). Got: system. To support different remap roles, add allowed_roles ["user", "assistant", "system", ...]
//   -->  client/remap_role_invalid_role.baml:8
//    |
//  7 |       user "human"
//  8 |       system "instructions"
//    |

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient = openai.OpenAiClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//     ╭─[client_remap_role_invalid_role.baml:1:1]
//   1 │ ╭─▶ client<llm> MyClient {
//   2 │ │     provider openai
//   3 │ │     options {
//   4 │ │       model "gpt-4o"
//   5 │ │       allowed_roles ["user", "assistant"]
//   6 │ │       remap_roles {
//   7 │ │         user "human"
//   8 │ │         system "instructions"
//   9 │ │       }
//  10 │ │     }
//  11 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
