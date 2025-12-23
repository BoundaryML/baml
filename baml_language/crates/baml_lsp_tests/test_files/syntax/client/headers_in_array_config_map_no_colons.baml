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
// Error: Expected config key, found LBrace
//    ╭─[ 0:7:7 ]
//    │
//  7 │       {
//    │       ┬  
//    │       ╰── Expected config key, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found Quote
//     ╭─[ 0:11:11 ]
//     │
//  11 │           "x-api-key" hi
//     │           ┬  
//     │           ╰── Expected config key, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found LBrace
//     ╭─[ 0:19:17 ]
//     │
//  19 │         headers {
//     │                 ┬  
//     │                 ╰── Expected config key, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found Quote
//     ╭─[ 0:20:11 ]
//     │
//  20 │           "x-api-key" hello
//     │           ┬  
//     │           ╰── Expected config key, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found RBracket
//     ╭─[ 0:26:5 ]
//     │
//  26 │     ]
//     │     ┬  
//     │     ╰── Expected config key, found RBracket
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:28:1 ]
//     │
//  28 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
