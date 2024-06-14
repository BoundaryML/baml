// TextComponentList.js
import React from 'react';
import SnippetCard from './SnippetCard';

const textComponents = [
  { id: "jinja_prompts", text1: 'jinja_prompts Text 1', text2: 'jinja_prompts Text 2' },
  { id: 1, text1: 'Second Component Text 1', text2: 'Second Component Text 2' },
  { id: 2, text1: 'Third Component Text 1', text2: 'Third Component Text 2' },
  // Add more components as needed
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
