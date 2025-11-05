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

