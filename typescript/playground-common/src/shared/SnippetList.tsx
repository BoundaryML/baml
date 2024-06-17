// TextComponentList.js
import React from 'react';
import SnippetCard from './SnippetCard';


const textComponents = [
  { 
    id: "system_user_prompts", 
    name: 'System vs user prompts',
    text1: `Configuring roles in LLM prompts enhances the effectiveness and reliability of interactions with language models. \nTry out {{  _.role() }} to get started`, 
    lang: 'baml',
    text2: 'function ClassifyMessage(input: string) -> Category {\n    client GPT4Turbo\n  \n    prompt #"\n      {# _.role("system") starts a system message #}\n      {{ _.role("system") }}\n  \n      Classify the following INPUT into ONE\n      of the following categories:\n  \n      {{ ctx.output_format }}\n  \n      {# This starts a user message #}\n      {{ _.role("user") }}\n  \n      INPUT: {{ input }}\n  \n      Response:\n    "#\n  }' 
  },
  { 
    id: "test_ai_function", 
    name: 'Test an AI function', 
    text1: `One important way to ensure your AI functions are working as expected is to write unit tests. This is especially important when you’re working with AI functions that are used in production, or when you’re working with a team.

    To test functions, create a test in a .baml file and run the test in the VSCode extension.`,
       
    lang: 'baml',
    text2: `test MyTest {
      functions [ExtractResume]
      args {
        resume_text "hello"
      }
    }
    
    `
  },
  { 
    id: "fall_back", 
    name: 'Fallbacks and redundancy', 
    text1: `Many LLMs are subject to fail due to transient errors. \nSetting up a fallback allows you to switch to a different LLM when prior LLMs fail (e.g. outage, high latency, rate limits, etc).`,
    lang: 'baml',
    text2: 'client<llm> MySafeClient {\n\tprovider baml-fallback\n\toptions {\n\t\t// First try GPT4 client, if it fails, try GPT35 client.\n\t\tstrategy [\n\t\t\tGPT4,\n\t\t\tGPT35\n\t\t\t// If you had more clients, you could add them here.\n\t\t\t// Anthropic\n\t\t]\n\t}\n}\n\nclient<llm> GPT4 {\n\tprovider baml-openai-chat\n\toptions {\n\t\t// ...\n\t}\n}\n\nclient<llm> GPT35 {\n\tprovider baml-openai-chat\n\toptions {\n\t\t// ...\n\t}\n}'

  },
  {
    id: "add_retries", 
    name: 'Retry policies', 
    text1: `Many LLMs are subject to fail due to transient errors. \nThe retry policy allows you to configure how many times and how the client should retry a failed operation before giving up.`,
    lang: 'baml',
    text2: 'retry_policy PolicyName {\n\tmax_retries int\n\tstrategy {\n\t\ttype constant_delay\n\t\tdelay_ms int? // defaults to 200\n\t} | {\n\t\ttype exponential_backoff\n\t\tdelay_ms int? // defaults to 200\n\t\tmax_delay_ms int? // defaults to 10000\n\t\tmultiplier float? // defaults to 1.5\n\t}\n}' 
  },
  {
    id: "evaluate_results",
    name: 'Evaluate LLM Results',
    text1: 'To add assertions to your tests, or add more complex testing scenarios, you can use pytest to test your functions, since Playground BAML tests don’t currently support assertions.', 
    lang: 'python',
    text2: 'from baml_client import baml as b\n  from baml_client.types import Email\n  from baml_client.testing import baml_test\n  import pytest\n\n  # Run `poetry run pytest -m baml_test` in this directory.\n  # Setup Boundary Studio to see test details!\n  @pytest.mark.asyncio\n  async def test_get_order_info():\n    order_info = await b.GetOrderInfo(Email(\n        subject="Order #1234",\n        body="Your order has been shipped. It will arrive on 1st Jan 2022. Product: iPhone 13. Cost: $999.99"\n    ))\n\n    assert order_info.cost == 999.99\n\n'
  },

  {
    id: "streaming_structured",
    name: 'Streaming Structured Data',
    text1: 'Streaming partial objects is useful if you want to start processing the response before it’s fully complete. You can stream anything from a string output type, to a complex object.', 
    lang: 'python',
    text2: `from baml_client import b

@app.get("/extract_resume")
async def extract_resume(resume_text: str):
    async def stream_resume(resume):
        stream = b.stream.ExtractResume(resume_text)
        async for chunk in stream:
            yield str(chunk.model_dump_json()) + "\\n"
                
    return StreamingResponse(stream_resume(resume), media_type="text/plain")
    `
  },


  
  // Add more components as neededstarting_page
];

const TextComponentList = ({ selectedId }: { selectedId: String }) => {
  const selectedComponent = textComponents.find(component => component.id === selectedId);

  return selectedComponent ? (
    <SnippetCard title={selectedComponent.name} description={selectedComponent.text1} code={selectedComponent.text2} lang={selectedComponent.lang} />
  ) : null;
};

export default TextComponentList;
