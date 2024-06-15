// TextComponentList.js
import React from 'react';
import SnippetCard from './SnippetCard';

const textComponents = [
  { id: "jinja_prompts", text1: 'Use roles to support chat message features in BAML!', text2: 'function ClassifyMessage(input: string) -> Category {\n    client GPT4Turbo\n  \n    prompt #"\n      {# _.role("system") starts a system message #}\n      {{ _.role("system") }}\n  \n      Classify the following INPUT into ONE\n      of the following categories:\n  \n      {{ ctx.output_format }}\n  \n      {# This starts a user message #}\n      {{ _.role("user") }}\n  \n      INPUT: {{ input }}\n  \n      Response:\n    "#\n  }' },
  { id: "dynamic_clients", text1: 'dynamic_clients Text 1', text2: 'dynamic_clients Text 2' },
  { id: "client_options", text1: 'Third client_options Text 1', text2: 'Third client_options Text 2' },
  { id: "starting_page", text1: 'SPs Text 1', text2: 'Third client_options Text 2' },
  // Add more components as neededstarting_page
];

const TextComponentList = ({ selectedId }: { selectedId: String }) => {
  const selectedComponent = textComponents.find(component => component.id === selectedId);

  if (!selectedComponent) {
    return selectedComponent ? (
      <SnippetCard text={"Welcome to BAML"} text2={"Dummy text"} />
    )  : null;
  }
  return selectedComponent ? (
    <SnippetCard text={selectedComponent.text1} text2={selectedComponent.text2} />
  ) : null;
};

export default TextComponentList;
