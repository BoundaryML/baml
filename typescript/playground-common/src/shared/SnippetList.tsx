// TextComponentList.js
import React from 'react';
import SnippetCard from './SnippetCard';


const textComponents = [
  { 
    id: "system_user_prompts", 
    name: 'System vs user prompts',
    text1: `Configuring roles in LLM prompts enhances the effectiveness and reliability of interactions with language models. Try out {{ _.role() to get started}}`, 
    text2: 'function ClassifyMessage(input: string) -> Category {\n    client GPT4Turbo\n  \n    prompt #"\n      {# _.role("system") starts a system message #}\n      {{ _.role("system") }}\n  \n      Classify the following INPUT into ONE\n      of the following categories:\n  \n      {{ ctx.output_format }}\n  \n      {# This starts a user message #}\n      {{ _.role("user") }}\n  \n      INPUT: {{ input }}\n  \n      Response:\n    "#\n  }' 
  },
  { 
    id: "test_ai_function", 
    name: 'Test an AI function', 
    text1: `There are two types of tests you may want to run on your AI functions: - Unit Tests: Tests a single AI function (using the playground) - Integration Tests: Tests a pipeline of AI functions and potentially business logic`, 
    text2: 'dynamic_clients Text 2' 
  },
  { 
    id: "fall_back", 
    name: 'Fallbacks and redundancy', 
    text1: `Many LLMs are subject to fail due to transient errors. \nSetting up a fallback allows you to switch to a different LLM when prior LLMs fail (e.g. outage, high latency, rate limits, etc).`,
    text2: 'client<llm> MySafeClient {\n\tprovider baml-fallback\n\toptions {\n\t\t// First try GPT4 client, if it fails, try GPT35 client.\n\t\tstrategy [\n\t\t\tGPT4,\n\t\t\tGPT35\n\t\t\t// If you had more clients, you could add them here.\n\t\t\t// Anthropic\n\t\t]\n\t}\n}\n\nclient<llm> GPT4 {\n\tprovider baml-openai-chat\n\toptions {\n\t\t// ...\n\t}\n}\n\nclient<llm> GPT35 {\n\tprovider baml-openai-chat\n\toptions {\n\t\t// ...\n\t}\n}'

  },
  {
    id: "add_retries", 
    name: 'Retry policies', 
    text1: `Many LLMs are subject to fail due to transient errors. \nThe retry policy allows you to configure how many times and how the client should retry a failed operation before giving up.`,
    text2: 'retry_policy PolicyName {\n\tmax_retries int\n\tstrategy {\n\t\ttype constant_delay\n\t\tdelay_ms int? // defaults to 200\n\t} | {\n\t\ttype exponential_backoff\n\t\tdelay_ms int? // defaults to 200\n\t\tmax_delay_ms int? // defaults to 10000\n\t\tmultiplier float? // defaults to 1.5\n\t}\n}' 
  },
  // Add more components as neededstarting_page
];

const TextComponentList = ({ selectedId }: { selectedId: String }) => {
  const selectedComponent = textComponents.find(component => component.id === selectedId);

  if (!selectedComponent) {
    return selectedComponent ? (
      <SnippetCard title='' description={"Welcome to BAML"} code={"Dummy text"} />
    )  : null;
  }
  return selectedComponent ? (
    <SnippetCard title={selectedComponent.name} description={selectedComponent.text1} code={selectedComponent.text2} />
  ) : null;
};

export default TextComponentList;
