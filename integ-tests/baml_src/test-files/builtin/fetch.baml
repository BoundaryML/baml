 // class Todo {
 //   id int
 //   todo string
 //   completed bool
 //   userId int
 // }

 // function GetTodo() -> Todo {
 //   Todo {
 //     id: 1,
 //   }
 // }

 // function LlmDescribeTodo(todo: Todo) -> string {
 //   client GPT4o
 //   prompt #"Describe the following todo in detail: {{ todo }}"#
 // }

 // function UseGetTodoFunction() -> string {
 //   let todo = GetTodo();
 //   LlmDescribeTodo(todo)
 // }

 // test UseGetTodoFunction() {
 //   functions [UseGetTodoFunction]
 //   args { }
 // }

 // function SearchWikipedia(query: string) -> string {
 //   baml.fetch_value<string>(baml.Request {
 //     base_url: UrlEncode("https://en.wikipedia.org/wiki/Special:Search", {search query})
 //   })
 // }

 // function UrlEncode(base_url: string, query_params: map<string, string>) -> string {
 //   client "openai/o3"
 //   prompt #"
 //     Encode the following base URL and query parameters into a valid URL:

 //     URL: {{ base_url }}

 //     Query Params: {{ query_params }}

 //     Answer with just the final URL, no other text
 //   "#
 // }

 // function ConvertUserQuery(query: string) -> string {
 //   client GPT4o
 //   prompt #"
 //     Convert the following user query into a search query for Wikipedia: {{ query }}

 //     Ideally it should be a single word that could map to an actual Wikipedia page
 //   "#
 // }

 // function LlmFormulateResponse(query: string, search_result: string?) -> string {
 //   client GPT4o
 //   prompt #"
 //     You are a helpful assistant. Answer the following user query: {{ query }}.

 //     {% if search_result != "" %}
 //     Use this Wikipedia search result as a reference for your answer: {{ search_result }}
 //     {% endif %}
 //   "#
 // }

 // function NeedsWikipediaSearch(query: string) -> bool {
 //   client GPT4o
 //   prompt #"Does the following user query need a Wikipedia search?: {{ query }}"#
 // }

 // function ChatResponse(query: string) -> string {
 //   let needs_search = NeedsWikipediaSearch(query);
 //   let search_result = SearchWikipedia(ConvertUserQuery(query));
 //   LlmFormulateResponse(query, search_result)
 // }

 // test TestName {
 //   functions [ChatResponse]
 //   args {
 //     query #"
 //       Tell me everything about Rome
 //     "#
 //   messages []
 //   }
 // }

 // test UrlEncode {
 //   functions [UrlEncode]
 //   args {
 //     base_url "https://en.wikipedia.org/wiki/Special:Search"
 //     query_params {search "Tell me everything about Rome"
 //     go "Go"}
 //     }
 // }

 // test Convert {
 //   functions [ConvertUserQuery]
 //   args {
 //     query #"
 //       Tell me everything about Rome
 //     "#
 //   }
 // }
