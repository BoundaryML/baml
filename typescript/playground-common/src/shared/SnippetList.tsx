// TextComponentList.js
import React from 'react';
import SnippetCard from './SnippetCard';

const textComponents = [
  { 
    id: "system_user_prompts", 
    name: 'System vs user prompts',
    text1: `Configuring roles in LLM prompts enhances the effectiveness and reliability of interactions with language models.`, 
    text2: 'function ClassifyMessage(input: string) -> Category {\n    client GPT4Turbo\n  \n    prompt #"\n      {# _.role("system") starts a system message #}\n      {{ _.role("system") }}\n  \n      Classify the following INPUT into ONE\n      of the following categories:\n  \n      {{ ctx.output_format }}\n  \n      {# This starts a user message #}\n      {{ _.role("user") }}\n  \n      INPUT: {{ input }}\n  \n      Response:\n    "#\n  }' 
  },
  { 
    id: "test_ai_function", 
    name: 'Test an AI function', 
    text1: `There are two types of tests you may want to run on your AI functions: - Unit Tests: Tests a single AI function (using the playground) - Integration Tests: Tests a pipeline of AI functions and potentially business logic`, 
    text2: 'dynamic_clients Text 2' 
  },
  { 
    id: "evaluate_results", 
    name: 'Evaluate results with assertions or LLM Evals', 
    text1: 'Third client_options Text 1', 
    text2: 'Third client_options Text 2' 
  },
  {
    id: "starting_page", 
    name: '', 
    text1: 'SPs Text 1', 
    text2: 'Third client_options Text 2' 
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
