client<llm> Testclient {
  provider "openai-responses"
  options {
    model "gpt-5-2025-08-07"
    api_key env.OPENAI_API_KEY
    tools [
      {
        server_label "parallel_web_search"
        server_url "https://mcp.parallel.ai/v1beta/search_mcp/"
        headers {
          "x-api-key" hi
        }
        type "mcp"
        require_approval "never"
      },
      {
        server_label "another_tool"
        server_url "https://example.com/tool"
        headers {
          "x-api-key" hello
          authorization "Bearer token"
        }
        type "mcp"
        require_approval "never"
      }
    ]
  }
}

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client Testclient = openai.OpenAiClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//     ╭─[client_headers_in_array_config_map_no_colons.baml:1:1]
//   1 │ ╭─▶ client<llm> Testclient {
//   2 │ │     provider "openai-responses"
//   3 │ │     options {
//   4 │ │       model "gpt-5-2025-08-07"
//   5 │ │       api_key env.OPENAI_API_KEY
//   6 │ │       tools [
//   7 │ │         {
//   8 │ │           server_label "parallel_web_search"
//   9 │ │           server_url "https://mcp.parallel.ai/v1beta/search_mcp/"
//  10 │ │           headers {
//  11 │ │             "x-api-key" hi
//  12 │ │           }
//  13 │ │           type "mcp"
//  14 │ │           require_approval "never"
//  15 │ │         },
//  16 │ │         {
//  17 │ │           server_label "another_tool"
//  18 │ │           server_url "https://example.com/tool"
//  19 │ │           headers {
//  20 │ │             "x-api-key" hello
//  21 │ │             authorization "Bearer token"
//  22 │ │           }
//  23 │ │           type "mcp"
//  24 │ │           require_approval "never"
//  25 │ │         }
//  26 │ │       ]
//  27 │ │     }
//  28 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
